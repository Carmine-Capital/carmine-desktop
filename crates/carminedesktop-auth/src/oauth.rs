use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use url::Url;

const SCOPES: &str = "User.Read Files.ReadWrite.All Sites.Read.All offline_access";

/// Minimal styled HTML prefix for the OAuth callback page shown in the browser.
const CALLBACK_HTML_PREFIX: &str = concat!(
    "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"UTF-8\">",
    "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
    "<title>Carmine Desktop</title>",
    "<style>",
    "body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;",
    "font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;",
    "background:#0f1117;color:#e4e4e7}",
    ".card{text-align:center;padding:3rem;border-radius:12px;",
    "background:#1a1b23;border:1px solid #2a2b35;max-width:400px}",
    "h1{margin:0 0 .5rem;font-size:1.5rem;color:#fff}",
    ".subtitle{color:#a1a1aa;margin:0;line-height:1.5}",
    ".brand{font-size:.85rem;color:#71717a;margin-bottom:1.5rem}",
    "</style></head><body><div class=\"card\">",
    "<div class=\"brand\">Carmine Desktop</div>",
);

pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

fn authority_url(tenant_id: Option<&str>) -> String {
    let tenant = tenant_id.unwrap_or("common");
    format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0")
}

fn generate_pkce() -> (String, String) {
    let mut rng = rand::rng();
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

/// Returns `(code, verifier, actual_port)`.
pub async fn run_pkce_flow(
    client_id: &str,
    tenant_id: Option<&str>,
    port: u16,
    opener: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
    url_tx: Option<tokio::sync::oneshot::Sender<String>>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> carminedesktop_core::Result<(String, String, u16)> {
    let (verifier, challenge) = generate_pkce();

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| {
            carminedesktop_core::Error::Auth(format!("failed to bind callback listener: {e}"))
        })?;

    let actual_port = listener
        .local_addr()
        .map_err(|e| {
            carminedesktop_core::Error::Auth(format!("failed to get listener address: {e}"))
        })?
        .port();

    let redirect_uri = format!("http://localhost:{actual_port}/callback");

    let mut auth_url = Url::parse(&format!("{}/authorize", authority_url(tenant_id)))
        .map_err(|e| carminedesktop_core::Error::Auth(format!("invalid authority URL: {e}")))?;

    auth_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    if let Some(tid) = tenant_id {
        auth_url.query_pairs_mut().append_pair("domain_hint", tid);
    }

    if let Some(tx) = url_tx
        && tx.send(auth_url.to_string()).is_err()
    {
        tracing::debug!("auth URL channel receiver already dropped");
    }

    tracing::info!("opening browser for authentication");
    match opener(auth_url.as_str()) {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!("failed to open browser: {e}");
            print_auth_url(auth_url.as_str());
        }
    }

    let code = wait_for_callback(listener, cancel_token).await?;

    Ok((code, verifier, actual_port))
}

async fn wait_for_callback(
    listener: tokio::net::TcpListener,
    cancel_token: tokio_util::sync::CancellationToken,
) -> carminedesktop_core::Result<String> {
    use http_body_util::Full;
    use hyper::body::Bytes;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    let timeout = tokio::time::Duration::from_secs(120);
    let (stream, _addr) = tokio::select! {
        result = tokio::time::timeout(timeout, listener.accept()) => {
            result
                .map_err(|_| carminedesktop_core::Error::Auth("authentication timed out after 120s".into()))?
                .map_err(|e| carminedesktop_core::Error::Auth(format!("callback accept failed: {e}")))?
        }
        _ = cancel_token.cancelled() => {
            return Err(carminedesktop_core::Error::Auth("sign-in cancelled".into()));
        }
    };

    let io = TokioIo::new(stream);
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = std::sync::Mutex::new(Some(tx));

    let service = service_fn(move |req: Request<hyper::body::Incoming>| {
        let tx = tx.lock().unwrap().take();
        async move {
            let query = req.uri().query().unwrap_or_default();
            let params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            if let Some(error) = params.iter().find(|(k, _)| k == "error") {
                let desc = params
                    .iter()
                    .find(|(k, _)| k == "error_description")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("unknown error");
                if let Some(tx) = tx {
                    let _ = tx.send(String::new());
                }
                let body = format!(
                    "{}<h1>Authentication Failed</h1><p class=\"subtitle\">{}: {desc}</p></div></body></html>",
                    CALLBACK_HTML_PREFIX, error.1
                );
                return Ok::<_, hyper::Error>(
                    Response::builder()
                        .header("Content-Type", "text/html")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap(),
                );
            }

            let code = params
                .iter()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();

            if let Some(tx) = tx {
                let _ = tx.send(code);
            }

            let body = format!(
                "{}<h1>Signed In</h1><p class=\"subtitle\">You can close this tab and return to Carmine Desktop.</p></div></body></html>",
                CALLBACK_HTML_PREFIX
            );
            Ok(Response::builder()
                .header("Content-Type", "text/html")
                .body(Full::new(Bytes::from(body)))
                .unwrap())
        }
    });

    tokio::spawn(async move {
        let _ = http1::Builder::new().serve_connection(io, service).await;
    });

    let code = rx
        .await
        .map_err(|_| carminedesktop_core::Error::Auth("callback channel closed".into()))?;

    if code.is_empty() {
        return Err(carminedesktop_core::Error::Auth(
            "authentication was denied or failed".into(),
        ));
    }

    Ok(code)
}

pub async fn exchange_code(
    client_id: &str,
    tenant_id: Option<&str>,
    code: &str,
    verifier: &str,
    port: u16,
) -> carminedesktop_core::Result<TokenResponse> {
    let redirect_uri = format!("http://localhost:{port}/callback");
    let token_url = format!("{}/token", authority_url(tenant_id));

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", verifier),
            ("scope", SCOPES),
        ])
        .send()
        .await
        .map_err(|e| {
            carminedesktop_core::Error::Auth(format!("token exchange request failed: {e}"))
        })?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(carminedesktop_core::Error::Auth(format!(
            "token exchange failed: {body}"
        )));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        carminedesktop_core::Error::Auth(format!("failed to parse token response: {e}"))
    })?;

    parse_token_response(&body)
}

pub async fn refresh_token(
    client_id: &str,
    tenant_id: Option<&str>,
    refresh_token: &str,
) -> carminedesktop_core::Result<TokenResponse> {
    let token_url = format!("{}/token", authority_url(tenant_id));

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPES),
        ])
        .send()
        .await
        .map_err(|e| {
            carminedesktop_core::Error::Auth(format!("token refresh request failed: {e}"))
        })?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("invalid_grant") {
            return Err(carminedesktop_core::Error::Auth(
                "refresh token expired or revoked — re-authentication required".into(),
            ));
        }
        return Err(carminedesktop_core::Error::Auth(format!(
            "token refresh failed: {body}"
        )));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        carminedesktop_core::Error::Auth(format!("failed to parse token response: {e}"))
    })?;

    parse_token_response(&body)
}

fn parse_token_response(body: &serde_json::Value) -> carminedesktop_core::Result<TokenResponse> {
    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| carminedesktop_core::Error::Auth("missing access_token in response".into()))?
        .to_string();

    let refresh = body["refresh_token"]
        .as_str()
        .ok_or_else(|| {
            carminedesktop_core::Error::Auth("missing refresh_token in response".into())
        })?
        .to_string();

    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    let expires_at = Utc::now() + Duration::seconds(expires_in);

    Ok(TokenResponse {
        access_token,
        refresh_token: refresh,
        expires_at,
    })
}

fn print_auth_url(url: &str) {
    eprintln!(
        "\nOpen this URL in your browser to sign in:\n\n  {url}\n\nWaiting for authentication...\n"
    );
}
