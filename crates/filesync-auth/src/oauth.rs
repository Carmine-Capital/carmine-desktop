use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use url::Url;

const SCOPES: &str = "User.Read Files.ReadWrite.All Sites.Read.All offline_access";

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

pub async fn run_pkce_flow(
    client_id: &str,
    tenant_id: Option<&str>,
    port: u16,
) -> filesync_core::Result<(String, String)> {
    let (verifier, challenge) = generate_pkce();

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| {
            filesync_core::Error::Auth(format!("failed to bind callback listener: {e}"))
        })?;

    let actual_port = listener
        .local_addr()
        .map_err(|e| filesync_core::Error::Auth(format!("failed to get listener address: {e}")))?
        .port();

    let redirect_uri = format!("http://localhost:{actual_port}/callback");

    let mut auth_url = Url::parse(&format!("{}/authorize", authority_url(tenant_id)))
        .map_err(|e| filesync_core::Error::Auth(format!("invalid authority URL: {e}")))?;

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

    tracing::info!("opening browser for authentication");
    open::that(auth_url.as_str())
        .map_err(|e| filesync_core::Error::Auth(format!("failed to open browser: {e}")))?;

    let code = wait_for_callback(listener).await?;

    Ok((code, verifier))
}

async fn wait_for_callback(listener: tokio::net::TcpListener) -> filesync_core::Result<String> {
    use http_body_util::Full;
    use hyper::body::Bytes;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    let timeout = tokio::time::Duration::from_secs(120);
    let (stream, _addr) = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| filesync_core::Error::Auth("authentication timed out after 120s".into()))?
        .map_err(|e| filesync_core::Error::Auth(format!("callback accept failed: {e}")))?;

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
                    "<html><body><h2>Authentication Failed</h2><p>{}: {desc}</p></body></html>",
                    error.1
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

            let body = "<html><body><h2>Authentication Successful</h2><p>You can close this window.</p></body></html>";
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
        .map_err(|_| filesync_core::Error::Auth("callback channel closed".into()))?;

    if code.is_empty() {
        return Err(filesync_core::Error::Auth(
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
) -> filesync_core::Result<TokenResponse> {
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
        .map_err(|e| filesync_core::Error::Auth(format!("token exchange request failed: {e}")))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(filesync_core::Error::Auth(format!(
            "token exchange failed: {body}"
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| filesync_core::Error::Auth(format!("failed to parse token response: {e}")))?;

    parse_token_response(&body)
}

pub async fn refresh_token(
    client_id: &str,
    tenant_id: Option<&str>,
    refresh_token: &str,
) -> filesync_core::Result<TokenResponse> {
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
        .map_err(|e| filesync_core::Error::Auth(format!("token refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("invalid_grant") {
            return Err(filesync_core::Error::Auth(
                "refresh token expired or revoked — re-authentication required".into(),
            ));
        }
        return Err(filesync_core::Error::Auth(format!(
            "token refresh failed: {body}"
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| filesync_core::Error::Auth(format!("failed to parse token response: {e}")))?;

    parse_token_response(&body)
}

fn parse_token_response(body: &serde_json::Value) -> filesync_core::Result<TokenResponse> {
    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| filesync_core::Error::Auth("missing access_token in response".into()))?
        .to_string();

    let refresh = body["refresh_token"]
        .as_str()
        .ok_or_else(|| filesync_core::Error::Auth("missing refresh_token in response".into()))?
        .to_string();

    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    let expires_at = Utc::now() + Duration::seconds(expires_in);

    Ok(TokenResponse {
        access_token,
        refresh_token: refresh,
        expires_at,
    })
}
