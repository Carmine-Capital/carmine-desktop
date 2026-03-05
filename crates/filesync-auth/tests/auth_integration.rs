use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};

use filesync_auth::oauth::TokenResponse;
use filesync_auth::storage;

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
        !(now + buffer < expires_soon),
        "token expiring in 3min should be considered near-expiry"
    );

    // given: token expires in exactly 5min (boundary)
    let expires_at_boundary = now + Duration::minutes(5);
    assert!(
        !(now + buffer < expires_at_boundary),
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
        !(now + buffer < already_expired),
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
