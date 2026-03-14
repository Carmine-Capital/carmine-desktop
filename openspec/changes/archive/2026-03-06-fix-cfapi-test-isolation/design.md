## Context

CfApi integration tests share a single sync root ID (`carminedesktop!<SID>!`) across all 6 tests. When `cargo test` runs them in parallel, only the first test to call `register()` gets a working sync root. The remaining tests call `is_registered()`, see `true`, skip registration, and connect to their own paths — but Windows never dispatches callbacks for those paths because the sync root is registered elsewhere. `Session::connect()` succeeds silently on non-registered paths, making the failure invisible at mount time and only surfacing as empty placeholders or "file not found" errors downstream.

The `cloud-filter` crate's `SyncRootIdBuilder` supports an `account_name` field that's currently unused. The sync root ID format is `provider-id!security-id!account-name`. By setting a unique `account_name` per mount, each mount gets its own sync root registration.

## Goals / Non-Goals

**Goals:**
- Each CfApi mount gets a unique sync root ID so multiple mounts (and parallel tests) never collide
- CfApi integration tests pass reliably on CI without `--test-threads=1`
- Production multi-drive mounts work correctly (each drive gets its own sync root)

**Non-Goals:**
- Changing FUSE mount behavior (Linux/macOS unaffected)
- Adding new CfApi features beyond the sync root isolation fix
- Changing the CfApi callback implementation or CoreOps logic

## Decisions

### D1: Use `account_name` field for sync root discrimination

The `SyncRootIdBuilder` already supports `.account_name()` which becomes the third component of the sync root ID: `carminedesktop!<SID>!<account_name>`.

**Alternatives considered:**
- **Separate provider name per mount** — Would work but provider name has a 255-char limit and is meant to identify the application, not individual mounts
- **External test serialization** (`--test-threads=1`) — Masks the bug instead of fixing it; doesn't help production multi-drive mounts

**Decision:** Parameterize `build_sync_root_id(account_name: &str)` and thread the value through `CfMountHandle::mount()`. In production, pass the drive ID. In tests, pass a unique per-test identifier.

### D2: Use nanos timestamp as test account name

Each test fixture already generates a unique nanos timestamp for its temp directory. Reuse this as the `account_name` for sync root isolation.

**Alternatives considered:**
- **UUID** — Adds a dependency for no benefit; nanos is already unique per test
- **Test function name** — Harder to thread through without macros

### D3: Add polling loop for placeholder assertion

The `cfapi_browse_populates_placeholders` test has no delay between `read_dir` and the assertion. Even with isolated sync roots, CfApi placeholder creation can be asynchronous on some Windows configurations. Replace the bare assertion with a retry loop (poll every 100ms, timeout after 2s).

**Alternatives considered:**
- **Longer fixed sleep** — Fragile; still fails on slow CI, wastes time on fast machines
- **No change** — Risk of flaky tests even after sync root fix

## Risks / Trade-offs

- [API surface change] `CfMountHandle::mount()` gains a required `account_name` parameter → callers in `carminedesktop-app` must be updated. Low risk since there's only one call site.
- [Sync root cleanup] Each test now registers a unique sync root that must be unregistered on teardown. Current teardown already calls `unmount()` → `unregister()`, so this is handled. Risk: if a test panics before teardown, orphaned sync roots accumulate. Mitigation: the `Drop` impl on `CfTestFixture` already handles this.
- [Polling in tests] The retry loop adds complexity but is strictly better than fixed sleeps or no-waits. Timeout is generous (2s) to avoid flakiness.
