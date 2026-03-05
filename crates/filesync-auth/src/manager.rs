use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

pub struct AuthManager {
    state: RwLock<AuthState>,
    client_id: String,
    tenant_id: Option<String>,
    redirect_port: u16,
}

#[derive(Debug, Default)]
struct AuthState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl AuthManager {
    pub fn new(client_id: String, tenant_id: Option<String>) -> Self {
        Self {
            state: RwLock::new(AuthState::default()),
            client_id,
            tenant_id,
            redirect_port: 0,
        }
    }

    pub async fn access_token(&self) -> filesync_core::Result<String> {
        let state = self.state.read().await;
        if let Some(ref token) = state.access_token {
            if let Some(expires_at) = state.expires_at {
                let buffer = chrono::Duration::minutes(5);
                if Utc::now() + buffer < expires_at {
                    return Ok(token.clone());
                }
            }
        }
        drop(state);
        self.refresh().await
    }

    pub async fn try_restore(&self, account_id: &str) -> filesync_core::Result<bool> {
        let tokens = match crate::storage::load_tokens(account_id)? {
            Some(t) => t,
            None => return Ok(false),
        };

        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);
        drop(state);

        let buffer = chrono::Duration::minutes(5);
        if chrono::Utc::now() + buffer < tokens.expires_at {
            return Ok(true);
        }

        match self.refresh().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("token restore: refresh failed: {e}");
                let mut state = self.state.write().await;
                state.access_token = None;
                state.refresh_token = None;
                state.expires_at = None;
                Ok(false)
            }
        }
    }

    pub async fn sign_in(&self) -> filesync_core::Result<()> {
        let (code, _verifier) = self.authorize().await?;
        self.exchange_code(&code, &_verifier).await
    }

    pub async fn sign_out(&self) -> filesync_core::Result<()> {
        let mut state = self.state.write().await;
        state.access_token = None;
        state.refresh_token = None;
        state.expires_at = None;

        if let Err(e) = crate::storage::delete_tokens(&self.client_id) {
            tracing::warn!("failed to delete stored tokens: {e}");
        }

        Ok(())
    }

    async fn authorize(&self) -> filesync_core::Result<(String, String)> {
        crate::oauth::run_pkce_flow(
            &self.client_id,
            self.tenant_id.as_deref(),
            self.redirect_port,
        )
        .await
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> filesync_core::Result<()> {
        let tokens = crate::oauth::exchange_code(
            &self.client_id,
            self.tenant_id.as_deref(),
            code,
            verifier,
            self.redirect_port,
        )
        .await?;

        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);

        crate::storage::store_tokens(&self.client_id, &tokens)?;

        Ok(())
    }

    async fn refresh(&self) -> filesync_core::Result<String> {
        let refresh_token = {
            let state = self.state.read().await;
            state
                .refresh_token
                .clone()
                .ok_or_else(|| filesync_core::Error::Auth("no refresh token available".into()))?
        };

        let tokens =
            crate::oauth::refresh_token(&self.client_id, self.tenant_id.as_deref(), &refresh_token)
                .await?;

        let access = tokens.access_token.clone();
        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);

        crate::storage::store_tokens(&self.client_id, &tokens)?;

        Ok(access)
    }
}
