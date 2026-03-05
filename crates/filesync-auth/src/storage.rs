use std::path::PathBuf;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::Utc;
use zeroize::Zeroizing;

use crate::oauth::TokenResponse;

const SERVICE_NAME: &str = "filesync";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub fn store_tokens(account_id: &str, tokens: &TokenResponse) -> filesync_core::Result<()> {
    let serialized = serialize_tokens(tokens)?;

    match keyring::Entry::new(SERVICE_NAME, account_id) {
        Ok(entry) => match entry.set_password(&serialized) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("keychain unavailable ({e}), using encrypted file fallback");
            }
        },
        Err(e) => {
            tracing::warn!("keyring init failed ({e}), using encrypted file fallback");
        }
    }

    store_tokens_encrypted(account_id, &serialized)
}

pub fn load_tokens(account_id: &str) -> filesync_core::Result<Option<TokenResponse>> {
    if let Some(tokens) = load_tokens_keyring(account_id)? {
        return Ok(Some(tokens));
    }

    load_tokens_encrypted(account_id)
}

pub fn delete_tokens(account_id: &str) -> filesync_core::Result<()> {
    if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, account_id) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                tracing::warn!("keychain delete failed: {e}");
            }
        }
    }

    let path = encrypted_token_path(account_id);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| filesync_core::Error::Auth(format!("failed to delete token file: {e}")))?;
    }

    Ok(())
}

fn serialize_tokens(tokens: &TokenResponse) -> filesync_core::Result<String> {
    let payload = serde_json::json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "expires_at": tokens.expires_at.to_rfc3339(),
    });

    serde_json::to_string(&payload)
        .map_err(|e| filesync_core::Error::Auth(format!("failed to serialize tokens: {e}")))
}

fn deserialize_tokens(data: &str) -> filesync_core::Result<TokenResponse> {
    let value: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| filesync_core::Error::Auth(format!("failed to parse stored tokens: {e}")))?;

    let access_token = value["access_token"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let refresh_token = value["refresh_token"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let expires_at = value["expires_at"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    Ok(TokenResponse {
        access_token,
        refresh_token,
        expires_at,
    })
}

fn load_tokens_keyring(account_id: &str) -> filesync_core::Result<Option<TokenResponse>> {
    let entry = match keyring::Entry::new(SERVICE_NAME, account_id) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };

    let password = match entry.get_password() {
        Ok(p) => p,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => {
            tracing::warn!("keychain read failed: {e}");
            return Ok(None);
        }
    };

    deserialize_tokens(&password).map(Some)
}

fn encrypted_token_path(account_id: &str) -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("filesync");
    config_dir.join(format!("tokens_{account_id}.enc"))
}

fn derive_key(password: &[u8], salt: &[u8]) -> filesync_core::Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(64 * 1024, 3, 1, Some(32))
        .map_err(|e| filesync_core::Error::Auth(format!("argon2 params error: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|e| filesync_core::Error::Auth(format!("key derivation failed: {e}")))?;
    Ok(key)
}

fn machine_password() -> Vec<u8> {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "filesync".to_string());
    let home = dirs::config_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("filesync-fallback-{username}@{home}").into_bytes()
}

fn store_tokens_encrypted(account_id: &str, serialized: &str) -> filesync_core::Result<()> {
    let path = encrypted_token_path(account_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| filesync_core::Error::Auth(format!("mkdir config dir failed: {e}")))?;
    }

    let password = machine_password();
    let salt: [u8; SALT_LEN] = rand::random();
    let key_bytes = derive_key(&password, &salt)?;
    let key = Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, serialized.as_bytes())
        .map_err(|_| filesync_core::Error::Auth("encryption failed".to_string()))?;

    let mut output = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);

    std::fs::write(&path, &output)
        .map_err(|e| filesync_core::Error::Auth(format!("write token file failed: {e}")))?;

    tracing::warn!("tokens stored in encrypted file (less secure than OS keychain)");
    Ok(())
}

fn load_tokens_encrypted(account_id: &str) -> filesync_core::Result<Option<TokenResponse>> {
    let path = encrypted_token_path(account_id);
    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read(&path)
        .map_err(|e| filesync_core::Error::Auth(format!("read token file failed: {e}")))?;

    let min_size = SALT_LEN + NONCE_LEN + 16;
    if data.len() < min_size {
        tracing::warn!("encrypted token file corrupt (too short), removing");
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }

    let (salt, rest) = data.split_at(SALT_LEN);
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

    let password = machine_password();
    let key_bytes = derive_key(&password, salt)?;
    let key = Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        filesync_core::Error::Auth("decryption failed (corrupted or wrong machine)".to_string())
    })?;

    let text = String::from_utf8(plaintext)
        .map_err(|e| filesync_core::Error::Auth(format!("invalid UTF-8 in tokens: {e}")))?;

    deserialize_tokens(&text).map(Some)
}
