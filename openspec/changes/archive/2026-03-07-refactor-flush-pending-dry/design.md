## Context

`MountHandle` (FUSE, `mount.rs`) and `CfMountHandle` (CfApi, `cfapi.rs`) both implement a private `flush_pending` method. The two implementations are functionally identical:

1. Call `writeback.list_pending()` to discover in-flight writes.
2. Filter to only entries for this handle's `drive_id`.
3. Run `graph.upload()` for each, with a 30-second `tokio::time::timeout`.
4. On success remove from writeback; on failure log and continue.

The method is pure business logic — it touches only `CacheManager`, `GraphClient`, `drive_id: String`, and a `tokio::runtime::Handle`. Neither FUSE nor CfApi primitives appear in it. The only surface difference between the two copies is the helper used to bridge async onto the sync call site:

- `mount.rs` uses `tokio::task::block_in_place(|| rt.block_on(...))`
- `cfapi.rs` uses a local `block_on_compat(&rt, ...)` helper

Both patterns do the same thing and can be unified under one call convention.

The `UNMOUNT_FLUSH_TIMEOUT` constant (30 s) is also independently declared in both files.

## Goals / Non-Goals

**Goals:**
- Single authoritative `flush_pending` implementation, callable from both `MountHandle::unmount` and `CfMountHandle::unmount`.
- `UNMOUNT_FLUSH_TIMEOUT` declared once.
- No change to observable behaviour (same timeout, same upload loop, same logging).
- All three platforms (Linux, macOS, Windows) compile without `#[cfg]` on the shared function.

**Non-Goals:**
- Changing the flush semantics (timeout value, retry policy, error handling).
- Unifying `MountHandle` and `CfMountHandle` behind a shared trait or enum (separate concern, not warranted by this change alone).
- Touching `shutdown_on_signal` (different function, each platform's version is tiny and typed differently — not worth unifying without the trait/enum work).

## Decisions

### D1 — Free async function in a new `pending.rs` module

**Chosen:** Add `crates/carminedesktop-vfs/src/pending.rs` exporting:

```rust
pub(crate) async fn flush_pending(
    cache: &CacheManager,
    graph: &GraphClient,
    drive_id: &str,
)
```

The module is not `#[cfg]`-gated. Both `mount.rs` and `cfapi.rs` import it with `use crate::pending::flush_pending`.

The callers (`MountHandle::unmount`, `CfMountHandle::unmount`) already use `block_in_place` / `block_on_compat` to bridge sync→async; they simply wrap the call to this function with that same bridge.

**Alternatives considered:**

- *Method on `CacheManager`*: Rejected — flushing uploads requires `GraphClient`, which `CacheManager` does not and should not know about (would introduce an upward dependency).
- *Free function in `core_ops.rs`*: `core_ops` is already large and focused on per-operation VFS logic. Unmount-time flush is lifecycle, not per-op logic. A dedicated module is cleaner.
- *Associated function on a new `PendingFlush` struct*: Unnecessary indirection for what is a single function.

### D2 — `UNMOUNT_FLUSH_TIMEOUT` declared in `pending.rs`

The constant moves to `pending.rs` (where the timeout is used) and is removed from both `mount.rs` and `cfapi.rs`. Both callers include the module so the constant is accessible if either needs to reference it for logging.

### D3 — `rt: &Handle` passed by the caller, not stored in `pending.rs`

The shared function is `async fn` — it does not need to know about `Handle`. The callers own the `block_on` bridge. This keeps `pending.rs` as pure async Rust with no sync/async bridging complexity.

## Risks / Trade-offs

- **Risk: divergence was intentional** — conceivably the two implementations were kept separate so they could diverge for platform-specific reasons in future. *Mitigation:* The function signature takes only platform-agnostic types (`CacheManager`, `GraphClient`, `&str`). If a platform ever needs different behaviour, the caller can skip the shared function and add its own logic — no harder than today.

- **Risk: `block_on_compat` vs `block_in_place`** — the two call sites use slightly different bridging helpers today. *Mitigation:* Both are equivalent for this use case (no nested `block_on`). The callers keep their own bridging; the shared function is pure `async fn` and is agnostic to which bridge is used.

## Migration Plan

1. Add `pending.rs` with `flush_pending` and `UNMOUNT_FLUSH_TIMEOUT`.
2. Declare `mod pending` in `lib.rs` (no `#[cfg]` gate).
3. In `mount.rs`: remove `UNMOUNT_FLUSH_TIMEOUT`, remove `flush_pending` body, call `self.rt.block_on(pending::flush_pending(...))` from `unmount` (wrapped in `block_in_place` as before).
4. In `cfapi.rs`: same — remove constant and body, call via `block_on_compat`.
5. `cargo build --all-targets` on all three platforms (or CI).
6. `cargo test --all-targets` — no behaviour changes expected so existing tests should pass unchanged.

No rollback concern — this is a pure internal refactor with no config, API, or protocol changes.

## Open Questions

_(none — scope is narrow and fully determined by the existing code.)_
