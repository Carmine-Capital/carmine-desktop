## Why

CfApi integration tests fail on Windows CI because all 6 tests share the same sync root ID (`CloudMount!<SID>!`) while running in parallel. Only the first test to register the sync root gets a working CfApi connection — the rest silently fail because `Session::connect()` succeeds even on non-registered paths, so callbacks are never dispatched. This causes 4 of 6 tests to fail with empty placeholders or "file not found" errors.

## What Changes

- Parameterize `build_sync_root_id()` to accept an `account_name` discriminator, producing unique sync root IDs per mount (format: `CloudMount!<SID>!<account_name>`)
- Thread the discriminator through `CfMountHandle::mount()` so each mount gets its own sync root
- Update CfApi integration tests to pass unique account names per test fixture
- Add a retry/poll loop for placeholder population in tests instead of fixed sleeps

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `virtual-filesystem`: CfApi mount now requires an `account_name` parameter to uniquely identify each sync root registration, enabling multiple concurrent sync roots per user

## Impact

- `crates/cloudmount-vfs/src/cfapi.rs` — `build_sync_root_id()` signature change, `CfMountHandle::mount()` gains `account_name` parameter
- `crates/cloudmount-vfs/tests/cfapi_integration.rs` — each test fixture passes a unique account name; browse test uses polling instead of no-wait
- `crates/cloudmount-app/src/main.rs` — callers of `CfMountHandle::mount()` must pass an account name (e.g., drive ID or mount label)
