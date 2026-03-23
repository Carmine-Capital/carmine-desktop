use std::path::PathBuf;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::Utc;
use zeroize::Zeroizing;

use crate::oauth::TokenResponse;

const SERVICE_NAME: &str = "carminedesktop";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub fn store_tokens(account_id: &str, tokens: &TokenResponse) -> carminedesktop_core::Result<()> {
    let serialized = serialize_tokens(tokens)?;

    match keyring::Entry::new(SERVICE_NAME, account_id) {
        Ok(entry) => match entry.set_password(&serialized) {
            Ok(()) => {
                // Verify with a fresh entry to catch backends that cache in-memory
                // but don't actually persist (e.g., kernel keyutils, locked collections)
                let verified =
                    keyring::Entry::new(SERVICE_NAME, account_id).and_then(|e| e.get_password());
                match verified {
                    Ok(ref readback) if readback == &serialized => return Ok(()),
                    Ok(_) => {
                        tracing::warn!(
                            "keychain verify failed (data mismatch), using encrypted file fallback"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "keychain verify failed ({e}), using encrypted file fallback"
                        );
                    }
                }
            }
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

pub fn load_tokens(account_id: &str) -> carminedesktop_core::Result<Option<TokenResponse>> {
    if let Some(tokens) = load_tokens_keyring(account_id)? {
        return Ok(Some(tokens));
    }

    load_tokens_encrypted(account_id)
}

pub fn delete_tokens(account_id: &str) -> carminedesktop_core::Result<()> {
    if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, account_id) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                tracing::warn!("keychain delete failed: {e}");
            }
        }
    }

    let path = encrypted_token_path(account_id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| {
            carminedesktop_core::Error::Auth(format!("failed to delete token file: {e}"))
        })?;
    }

    Ok(())
}

fn serialize_tokens(tokens: &TokenResponse) -> carminedesktop_core::Result<String> {
    let payload = serde_json::json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "expires_at": tokens.expires_at.to_rfc3339(),
    });

    serde_json::to_string(&payload)
        .map_err(|e| carminedesktop_core::Error::Auth(format!("failed to serialize tokens: {e}")))
}

fn deserialize_tokens(data: &str) -> carminedesktop_core::Result<TokenResponse> {
    let value: serde_json::Value = serde_json::from_str(data).map_err(|e| {
        carminedesktop_core::Error::Auth(format!("failed to parse stored tokens: {e}"))
    })?;

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

fn load_tokens_keyring(account_id: &str) -> carminedesktop_core::Result<Option<TokenResponse>> {
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

/// Sanitize account_id for use in filenames — prevent path traversal.
fn sanitize_account_id(account_id: &str) -> String {
    account_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '@' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn encrypted_token_path(account_id: &str) -> carminedesktop_core::Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| {
            carminedesktop_core::Error::Auth(
                "no config directory available (dirs::config_dir returned None)".to_string(),
            )
        })?
        .join("carminedesktop");
    let safe_id = sanitize_account_id(account_id);
    Ok(config_dir.join(format!("tokens_{safe_id}.enc")))
}

fn derive_key(password: &[u8], salt: &[u8]) -> carminedesktop_core::Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(64 * 1024, 3, 1, Some(32))
        .map_err(|e| carminedesktop_core::Error::Auth(format!("argon2 params error: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|e| carminedesktop_core::Error::Auth(format!("key derivation failed: {e}")))?;
    Ok(key)
}

/// Read platform-specific machine ID for use as key derivation entropy.
#[cfg(target_os = "linux")]
fn machine_id() -> Option<String> {
    std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
fn machine_id() -> Option<String> {
    std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Some(rest) = line.trim().strip_prefix("\"IOPlatformUUID\"")
                    && let Some(value) = rest.split('"').nth(1)
                    && !value.is_empty()
                {
                    return Some(value.to_string());
                }
            }
            None
        })
}

#[cfg(target_os = "windows")]
fn machine_id() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[0] == "MachineGuid" {
                    return Some(parts[2].to_string());
                }
            }
            None
        })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn machine_id() -> Option<String> {
    None
}

fn machine_password() -> Vec<u8> {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "carminedesktop".to_string());
    let home = dirs::config_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mid = machine_id().unwrap_or_default();
    format!("carminedesktop-fallback-{username}@{home}:{mid}").into_bytes()
}

fn store_tokens_encrypted(account_id: &str, serialized: &str) -> carminedesktop_core::Result<()> {
    let path = encrypted_token_path(account_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            carminedesktop_core::Error::Auth(format!("mkdir config dir failed: {e}"))
        })?;
    }

    let password = machine_password();
    let salt: [u8; SALT_LEN] = rand::random();
    let key_bytes = derive_key(&password, &salt)?;
    let key = Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, serialized.as_bytes())
        .map_err(|_| carminedesktop_core::Error::Auth("encryption failed".to_string()))?;

    let mut output = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);

    // Write with restrictive permissions: 0600 on Unix (owner read/write only).
    // On Windows, %APPDATA% default ACL is already user-only.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| {
                carminedesktop_core::Error::Auth(format!("write token file failed: {e}"))
            })?;
        file.write_all(&output).map_err(|e| {
            carminedesktop_core::Error::Auth(format!("write token file failed: {e}"))
        })?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, &output).map_err(|e| {
            carminedesktop_core::Error::Auth(format!("write token file failed: {e}"))
        })?;
    }

    tracing::warn!("tokens stored in encrypted file (less secure than OS keychain)");
    Ok(())
}

fn load_tokens_encrypted(account_id: &str) -> carminedesktop_core::Result<Option<TokenResponse>> {
    let path = encrypted_token_path(account_id)?;
    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read(&path)
        .map_err(|e| carminedesktop_core::Error::Auth(format!("read token file failed: {e}")))?;

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
        carminedesktop_core::Error::Auth(
            "decryption failed (corrupted or wrong machine)".to_string(),
        )
    })?;

    let text = String::from_utf8(plaintext)
        .map_err(|e| carminedesktop_core::Error::Auth(format!("invalid UTF-8 in tokens: {e}")))?;

    deserialize_tokens(&text).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_account_id_strips_path_traversal() {
        assert_eq!(sanitize_account_id("normal-user_123"), "normal-user_123");
        assert_eq!(sanitize_account_id("user@domain.com"), "user@domain.com");
        assert_eq!(sanitize_account_id("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_account_id("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_account_id(""), "");
    }
}
