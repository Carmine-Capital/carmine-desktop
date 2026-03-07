use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct AuthManager {
    state: RwLock<AuthState>,
    client_id: String,
    tenant_id: Option<String>,
    redirect_port: u16,
    opener: OpenerFn,
}

#[derive(Debug, Default)]
struct AuthState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl AuthManager {
    pub fn new(client_id: String, tenant_id: Option<String>, opener: OpenerFn) -> Self {
        Self {
            state: RwLock::new(AuthState::default()),
            client_id,
            tenant_id,
            redirect_port: 0,
            opener,
        }
    }

    pub async fn access_token(&self) -> cloudmount_core::Result<String> {
        let state = self.state.read().await;
        if let Some(ref token) = state.access_token
            && let Some(expires_at) = state.expires_at
        {
            let buffer = chrono::Duration::minutes(5);
            if Utc::now() + buffer < expires_at {
                return Ok(token.clone());
            }
        }
        drop(state);
        self.refresh().await
    }

    pub async fn try_restore(&self, _account_id: &str) -> cloudmount_core::Result<bool> {
        let tokens = match crate::storage::load_tokens(&self.client_id)? {
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

    pub async fn sign_in(
        &self,
        url_tx: Option<tokio::sync::oneshot::Sender<String>>,
    ) -> cloudmount_core::Result<()> {
        let (code, verifier, actual_port) = self.authorize(url_tx).await?;
        self.exchange_code(&code, &verifier, actual_port).await
    }

    pub async fn sign_out(&self) -> cloudmount_core::Result<()> {
        let mut state = self.state.write().await;
        state.access_token = None;
        state.refresh_token = None;
        state.expires_at = None;

        if let Err(e) = crate::storage::delete_tokens(&self.client_id) {
            tracing::warn!("failed to delete stored tokens: {e}");
        }

        Ok(())
    }

    async fn authorize(
        &self,
        url_tx: Option<tokio::sync::oneshot::Sender<String>>,
    ) -> cloudmount_core::Result<(String, String, u16)> {
        crate::oauth::run_pkce_flow(
            &self.client_id,
            self.tenant_id.as_deref(),
            self.redirect_port,
            self.opener.as_ref(),
            url_tx,
        )
        .await
    }

    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        actual_port: u16,
    ) -> cloudmount_core::Result<()> {
        let tokens = crate::oauth::exchange_code(
            &self.client_id,
            self.tenant_id.as_deref(),
            code,
            verifier,
            actual_port,
        )
        .await?;

        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);

        crate::storage::store_tokens(&self.client_id, &tokens)?;

        Ok(())
    }

    async fn refresh(&self) -> cloudmount_core::Result<String> {
        let refresh_token = {
            let state = self.state.read().await;
            state
                .refresh_token
                .clone()
                .ok_or_else(|| cloudmount_core::Error::Auth("no refresh token available".into()))?
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
