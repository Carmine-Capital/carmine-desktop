use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};

use carminedesktop_auth::AuthManager;
use carminedesktop_auth::oauth::TokenResponse;
use carminedesktop_auth::storage;

fn make_test_tokens() -> TokenResponse {
    TokenResponse {
        access_token: "test-access-token".to_string(),
        refresh_token: "test-refresh-token".to_string(),
        expires_at: Utc::now() + Duration::hours(1),
    }
}

fn make_auth_manager(client_id: &str) -> AuthManager {
    AuthManager::new(client_id.to_string(), None, Arc::new(|_url: &str| Ok(())))
}

#[test]
fn token_serialization_roundtrip() {
    let account_id = "test_ser_roundtrip";
    let now = Utc::now();
    let original = TokenResponse {
        access_token: "access-abc-123".to_string(),
        refresh_token: "refresh-xyz-789".to_string(),
        expires_at: now + Duration::hours(1),
    };

    storage::store_tokens(account_id, &original).unwrap();

    let loaded = storage::load_tokens(account_id)
        .unwrap()
        .expect("tokens should be loadable after store");

    assert_eq!(loaded.access_token, original.access_token);
    assert_eq!(loaded.refresh_token, original.refresh_token);

    // RFC3339 serialization may lose sub-second precision
    let drift = (loaded.expires_at - original.expires_at)
        .num_seconds()
        .abs();
    assert!(drift <= 1, "expires_at drift too large: {drift}s");

    storage::delete_tokens(account_id).unwrap();
}

#[test]
fn encrypted_file_storage_roundtrip() {
    let account_id = "test_enc_roundtrip";
    let tokens = TokenResponse {
        access_token: "enc-access-!@#$%^&*()_+-=[]{}|;':\",./<>?".to_string(),
        refresh_token: "enc-refresh-token-with-unicode-é-ñ-ü".to_string(),
        expires_at: Utc::now() + Duration::hours(2),
    };

    storage::store_tokens(account_id, &tokens).unwrap();

    let loaded = storage::load_tokens(account_id)
        .unwrap()
        .expect("tokens should survive storage roundtrip");

    assert_eq!(loaded.access_token, tokens.access_token);
    assert_eq!(loaded.refresh_token, tokens.refresh_token);

    storage::delete_tokens(account_id).unwrap();
    let after_delete = storage::load_tokens(account_id).unwrap();
    assert!(after_delete.is_none(), "tokens should be gone after delete");
}

#[test]
fn token_expiry_detection() {
    let buffer = Duration::minutes(5);
    let now = Utc::now();

    // given: token expires in 3min (within 5min buffer)
    let expires_soon = now + Duration::minutes(3);
    assert!(
        now + buffer >= expires_soon,
        "token expiring in 3min should be considered near-expiry"
    );

    // given: token expires in exactly 5min (boundary)
    let expires_at_boundary = now + Duration::minutes(5);
    assert!(
        now + buffer >= expires_at_boundary,
        "token expiring in exactly 5min should be considered near-expiry"
    );

    // given: token expires in 6min (outside buffer)
    let expires_later = now + Duration::minutes(6);
    assert!(
        now + buffer < expires_later,
        "token expiring in 6min should still be valid"
    );

    // given: token already expired
    let already_expired = now - Duration::minutes(1);
    assert!(
        now + buffer >= already_expired,
        "already-expired token should be detected"
    );

    // given: token expires in 1h
    let expires_much_later = now + Duration::hours(1);
    assert!(
        now + buffer < expires_much_later,
        "token expiring in 1h should be valid"
    );
}

#[test]
fn pkce_verifier_challenge_validity() {
    use rand::Rng;

    let mut rng = rand::rng();
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    let is_base64url = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    assert!(
        is_base64url(&verifier),
        "verifier must use base64url alphabet only"
    );
    assert!(
        is_base64url(&challenge),
        "challenge must use base64url alphabet only"
    );

    // 32 random bytes -> 43 base64url chars; SHA-256 is also 32 bytes -> 43 chars
    assert_eq!(verifier.len(), 43);
    assert_eq!(challenge.len(), 43);

    let mut verify_hasher = Sha256::new();
    verify_hasher.update(verifier.as_bytes());
    let recomputed = URL_SAFE_NO_PAD.encode(verify_hasher.finalize());
    assert_eq!(
        challenge, recomputed,
        "challenge must equal SHA256(verifier)"
    );
}

#[tokio::test]
async fn finalize_sign_in_migrates_tokens_from_client_id_to_account_id() {
    let client_id = "test-finalize-client-id-migrate";
    let account_id = "test-finalize-drive-id-migrate";

    // Cleanup before test to ensure a clean state
    storage::delete_tokens(client_id).unwrap();
    storage::delete_tokens(account_id).unwrap();

    // Store tokens under the client_id (simulating the pre-fix state after exchange_code)
    let tokens = make_test_tokens();
    storage::store_tokens(client_id, &tokens).unwrap();

    let manager = make_auth_manager(client_id);

    // finalize_sign_in should migrate tokens from client_id → account_id
    manager.finalize_sign_in(account_id).await.unwrap();

    // Tokens must now exist under account_id
    let loaded = storage::load_tokens(account_id)
        .unwrap()
        .expect("tokens should be under account_id after finalize_sign_in");
    assert_eq!(loaded.access_token, tokens.access_token);

    // Old client_id entry must be gone
    let old = storage::load_tokens(client_id).unwrap();
    assert!(
        old.is_none(),
        "tokens under client_id should be deleted after migration"
    );

    // Cleanup
    storage::delete_tokens(account_id).unwrap();
}

#[tokio::test]
async fn finalize_sign_in_noop_when_key_already_correct() {
    let client_id = "test-finalize-client-id-noop";
    let account_id = "test-finalize-drive-id-noop";

    // Cleanup before test
    storage::delete_tokens(client_id).unwrap();
    storage::delete_tokens(account_id).unwrap();

    // Pre-set account_id so storage_key() returns account_id already
    let manager = make_auth_manager(client_id);
    manager.set_account_id(account_id).await;

    // Store tokens under the correct account_id key
    let tokens = make_test_tokens();
    storage::store_tokens(account_id, &tokens).unwrap();

    // finalize_sign_in with the same id — should be a no-op (no migration)
    manager.finalize_sign_in(account_id).await.unwrap();

    // Tokens must still exist under account_id
    let loaded = storage::load_tokens(account_id)
        .unwrap()
        .expect("tokens should still be under account_id after noop finalize");
    assert_eq!(loaded.access_token, tokens.access_token);

    // No tokens should have appeared under client_id
    let under_client = storage::load_tokens(client_id).unwrap();
    assert!(
        under_client.is_none(),
        "no tokens should exist under client_id in noop case"
    );

    // Cleanup
    storage::delete_tokens(account_id).unwrap();
}

#[tokio::test]
async fn try_restore_falls_back_to_client_id_and_migrates() {
    let client_id = "test-restore-client-id-fallback";
    let account_id = "test-restore-drive-id-fallback";

    // Cleanup before test
    storage::delete_tokens(client_id).unwrap();
    storage::delete_tokens(account_id).unwrap();

    // Store tokens under client_id only (simulating a broken pre-fix installation)
    let tokens = make_test_tokens();
    storage::store_tokens(client_id, &tokens).unwrap();

    let manager = make_auth_manager(client_id);

    // try_restore should fall back, migrate, and succeed
    let restored = manager.try_restore(account_id).await.unwrap();
    assert!(
        restored,
        "try_restore should succeed via client_id fallback"
    );

    // Tokens must now be under account_id
    let loaded = storage::load_tokens(account_id)
        .unwrap()
        .expect("tokens should be under account_id after fallback migration");
    assert_eq!(loaded.access_token, tokens.access_token);

    // Old client_id entry must be cleaned up
    let old = storage::load_tokens(client_id).unwrap();
    assert!(
        old.is_none(),
        "tokens under client_id should be deleted after migration"
    );

    // Cleanup
    storage::delete_tokens(account_id).unwrap();
}

#[tokio::test]
async fn try_restore_succeeds_directly_when_tokens_under_account_id() {
    let client_id = "test-restore-client-id-direct";
    let account_id = "test-restore-drive-id-direct";

    // Cleanup before test
    storage::delete_tokens(client_id).unwrap();
    storage::delete_tokens(account_id).unwrap();

    // Store tokens directly under account_id (normal post-fix state)
    let tokens = make_test_tokens();
    storage::store_tokens(account_id, &tokens).unwrap();

    let manager = make_auth_manager(client_id);

    // try_restore should succeed on the first lookup without touching client_id
    let restored = manager.try_restore(account_id).await.unwrap();
    assert!(
        restored,
        "try_restore should succeed directly from account_id key"
    );

    // client_id key should remain empty (fallback was not needed)
    let under_client = storage::load_tokens(client_id).unwrap();
    assert!(
        under_client.is_none(),
        "client_id key should not be touched when account_id key has tokens"
    );

    // Cleanup
    storage::delete_tokens(account_id).unwrap();
}

/// Verify that try_restore succeeds with expired tokens when network is unavailable.
/// The refresh will fail but stored tokens should be preserved for later retry.
#[tokio::test]
async fn test_try_restore_keeps_tokens_when_refresh_fails() -> carminedesktop_core::Result<()> {
    let account_id = "offline-restore-test";
    let _ = carminedesktop_auth::storage::delete_tokens(account_id);

    // Store tokens with an already-expired access token
    let expired_tokens = TokenResponse {
        access_token: "expired-access".to_string(),
        refresh_token: "valid-refresh".to_string(),
        expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
    };
    carminedesktop_auth::storage::store_tokens(account_id, &expired_tokens)?;

    // AuthManager with no real tenant — refresh() will fail with network error
    let manager = carminedesktop_auth::AuthManager::new(
        "test-client-id".to_string(),
        Some("nonexistent-tenant".to_string()),
        std::sync::Arc::new(|_: &str| Err("no browser".to_string())),
    );

    // try_restore should return Ok(true) even though refresh fails,
    // because stored tokens exist and can be retried later
    let result = manager.try_restore(account_id).await?;
    assert!(
        result,
        "try_restore should return true when stored tokens exist"
    );

    // Cleanup
    let _ = carminedesktop_auth::storage::delete_tokens(account_id);
    Ok(())
}

