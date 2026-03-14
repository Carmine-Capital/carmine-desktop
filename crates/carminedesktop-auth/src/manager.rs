use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct AuthManager {
    state: RwLock<AuthState>,
    client_id: String,
    tenant_id: Option<String>,
    redirect_port: u16,
    opener: OpenerFn,
    active_cancel: Arc<Mutex<Option<CancellationToken>>>,
}

#[derive(Debug, Default)]
struct AuthState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    account_id: Option<String>,
}

impl AuthManager {
    pub fn new(client_id: String, tenant_id: Option<String>, opener: OpenerFn) -> Self {
        Self {
            state: RwLock::new(AuthState::default()),
            client_id,
            tenant_id,
            redirect_port: 0,
            opener,
            active_cancel: Arc::new(Mutex::new(None)),
        }
    }

    pub fn cancel(&self) {
        let mut guard = self.active_cancel.lock().unwrap();
        if let Some(token) = guard.take() {
            token.cancel();
        }
    }

    /// Set the account_id for token storage. Call after discovering the user
    /// identity (e.g., from Graph API) so that subsequent store/delete
    /// operations use the correct identifier.
    pub async fn set_account_id(&self, id: &str) {
        let mut state = self.state.write().await;
        state.account_id = Some(id.to_string());
    }

    /// Finalizes sign-in by setting the account_id and migrating any tokens
    /// previously stored under the client_id key to the correct account_id key.
    ///
    /// Uses store-then-delete ordering: the new key is written first, and the
    /// old key is only deleted on success. On partial failure, the old entry is
    /// preserved and no tokens are lost.
    pub async fn finalize_sign_in(&self, id: &str) -> carminedesktop_core::Result<()> {
        let old_key = self.storage_key().await;

        {
            let mut state = self.state.write().await;
            state.account_id = Some(id.to_string());
        }

        if old_key == id {
            return Ok(());
        }

        if let Some(tokens) = crate::storage::load_tokens(&old_key)? {
            crate::storage::store_tokens(id, &tokens)?;
            crate::storage::delete_tokens(&old_key)?;
        }

        Ok(())
    }

    /// Returns the storage key for token operations: account_id if set,
    /// otherwise falls back to client_id for backward compatibility.
    async fn storage_key(&self) -> String {
        let state = self.state.read().await;
        state
            .account_id
            .clone()
            .unwrap_or_else(|| self.client_id.clone())
    }

    pub async fn access_token(&self) -> carminedesktop_core::Result<String> {
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

    pub async fn try_restore(&self, account_id: &str) -> carminedesktop_core::Result<bool> {
        let tokens = match crate::storage::load_tokens(account_id)? {
            Some(t) => t,
            None => {
                // Fallback: tokens may be stored under client_id from a pre-fix sign-in.
                // Migrate on success to repair existing broken installations.
                match crate::storage::load_tokens(&self.client_id)? {
                    Some(t) => {
                        crate::storage::store_tokens(account_id, &t)?;
                        crate::storage::delete_tokens(&self.client_id)?;
                        tracing::info!("migrated tokens from client_id key to account_id key");
                        t
                    }
                    None => return Ok(false),
                }
            }
        };

        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);
        state.account_id = Some(account_id.to_string());
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
    ) -> carminedesktop_core::Result<()> {
        {
            let mut guard = self.active_cancel.lock().unwrap();
            if let Some(old) = guard.take() {
                old.cancel();
            }
            *guard = Some(CancellationToken::new());
        }
        let result = self.authorize(url_tx).await;
        self.active_cancel.lock().unwrap().take();
        let (code, verifier, actual_port) = result?;
        self.exchange_code(&code, &verifier, actual_port).await
    }

    pub async fn sign_out(&self) -> carminedesktop_core::Result<()> {
        let storage_key = self.storage_key().await;

        let mut state = self.state.write().await;
        state.access_token = None;
        state.refresh_token = None;
        state.expires_at = None;
        state.account_id = None;

        if let Err(e) = crate::storage::delete_tokens(&storage_key) {
            tracing::warn!("failed to delete stored tokens: {e}");
        }

        Ok(())
    }

    async fn authorize(
        &self,
        url_tx: Option<tokio::sync::oneshot::Sender<String>>,
    ) -> carminedesktop_core::Result<(String, String, u16)> {
        let cancel_token = {
            let guard = self.active_cancel.lock().unwrap();
            guard.as_ref().map(|t| t.child_token()).unwrap_or_default()
        };
        crate::oauth::run_pkce_flow(
            &self.client_id,
            self.tenant_id.as_deref(),
            self.redirect_port,
            self.opener.as_ref(),
            url_tx,
            cancel_token,
        )
        .await
    }

    async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        actual_port: u16,
    ) -> carminedesktop_core::Result<()> {
        let tokens = crate::oauth::exchange_code(
            &self.client_id,
            self.tenant_id.as_deref(),
            code,
            verifier,
            actual_port,
        )
        .await?;

        let storage_key = self.storage_key().await;

        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);

        crate::storage::store_tokens(&storage_key, &tokens)?;

        Ok(())
    }

    async fn refresh(&self) -> carminedesktop_core::Result<String> {
        let refresh_token = {
            let state = self.state.read().await;
            state.refresh_token.clone().ok_or_else(|| {
                carminedesktop_core::Error::Auth("no refresh token available".into())
            })?
        };

        let tokens =
            crate::oauth::refresh_token(&self.client_id, self.tenant_id.as_deref(), &refresh_token)
                .await?;

        let storage_key = self.storage_key().await;

        let access = tokens.access_token.clone();
        let mut state = self.state.write().await;
        state.access_token = Some(tokens.access_token.clone());
        state.refresh_token = Some(tokens.refresh_token.clone());
        state.expires_at = Some(tokens.expires_at);

        crate::storage::store_tokens(&storage_key, &tokens)?;

        Ok(access)
    }
}
