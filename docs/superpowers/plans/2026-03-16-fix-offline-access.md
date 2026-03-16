# Fix Offline Access Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make pinned offline folders accessible when network is unavailable, both when going offline while the app is running and when starting the app offline.

**Architecture:** Three-layer fix: (1) Auth preserves tokens on refresh failure so mounts can start offline, (2) Mount startup falls back to SQLite-cached root item instead of requiring Graph API, (3) VFS uses a shared `Arc<AtomicBool>` offline flag to skip all Graph API calls and serve exclusively from cache. Delta sync clears the flag when network returns.

**Tech Stack:** Rust, tokio, AtomicBool, wiremock (tests)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/carminedesktop-auth/src/manager.rs` | Modify | Don't clear tokens when refresh fails in `try_restore()` |
| `crates/carminedesktop-vfs/src/core_ops.rs` | Modify | Add offline flag; cache-only mode in VFS operations |
| `crates/carminedesktop-vfs/src/mount.rs` | Modify | Offline flag param + SQLite fallback for root item |
| `crates/carminedesktop-vfs/src/fuse_fs.rs` | Modify | Thread offline flag through `CarmineDesktopFs::new` |
| `crates/carminedesktop-vfs/src/winfsp_fs.rs` | Modify | Thread offline flag through `WinFspMountHandle::mount` |
| `crates/carminedesktop-app/src/main.rs` | Modify | Network error tolerance in mount validation + delta sync; wire offline flag |
| `crates/carminedesktop-auth/tests/auth_integration.rs` | Modify | Test try_restore offline tolerance |
| `crates/carminedesktop-vfs/tests/offline_vfs_tests.rs` | Create | Test VFS cache-only mode when offline flag is set |

---

## Chunk 1: Auth and Mount Startup Offline Tolerance

### Task 1: Fix `try_restore()` to preserve tokens on refresh failure

**Files:**
- Modify: `crates/carminedesktop-auth/src/manager.rs:132-142`
- Modify: `crates/carminedesktop-auth/tests/auth_integration.rs`

**Root cause:** When `refresh()` fails (network down), lines 136-139 clear `access_token`, `refresh_token`, and `expires_at` from memory, then return `Ok(false)`. This prevents mounts from ever starting.

**Fix:** When refresh fails but stored tokens exist, keep them in memory and return `Ok(true)`. The access token is expired but the refresh token is still valid for later. VFS will serve from cache; delta sync will retry refresh when network returns.

**Edge case considered:** `access_token()` will call `refresh()` again on Graph API calls (since the token is expired). This will fail with `Error::Network` or `Error::Auth`, which VFS catches and falls back to cache. No infinite loop — `refresh()` is called once and returns.

- [ ] **Step 1: Write failing test for offline token restore**

In `crates/carminedesktop-auth/tests/auth_integration.rs`, add:

```rust
/// Verify that try_restore succeeds with expired tokens when network is unavailable.
/// The refresh will fail but stored tokens should be preserved for later retry.
#[tokio::test]
async fn test_try_restore_keeps_tokens_when_refresh_fails() -> carminedesktop_core::Result<()> {
    use carminedesktop_auth::oauth::TokenResponse;

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
    assert!(result, "try_restore should return true when stored tokens exist");

    // Cleanup
    let _ = carminedesktop_auth::storage::delete_tokens(account_id);
    Ok(())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `make test-crate CRATE=carminedesktop-auth TEST=test_try_restore_keeps_tokens_when_refresh_fails`
Expected: FAIL — `try_restore` currently returns `false` and clears tokens.

- [ ] **Step 3: Implement fix in try_restore**

In `crates/carminedesktop-auth/src/manager.rs`, change lines 132-142 from:

```rust
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
```

To:

```rust
        match self.refresh().await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!(
                    "token restore: refresh failed (offline?): {e} — \
                     keeping stored tokens for later retry"
                );
                // Do NOT clear tokens — the refresh token is still valid and
                // can be retried when the network returns. Clearing them would
                // prevent mounts from starting in offline mode.
                Ok(true)
            }
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `make test-crate CRATE=carminedesktop-auth TEST=test_try_restore_keeps_tokens_when_refresh_fails`
Expected: PASS

- [ ] **Step 5: Run full auth test suite**

Run: `make test-crate CRATE=carminedesktop-auth`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/carminedesktop-auth/src/manager.rs crates/carminedesktop-auth/tests/auth_integration.rs
git commit -m "fix(auth): preserve tokens when refresh fails in try_restore

When offline, refresh() fails but the stored refresh token is still
valid. Previously we cleared all tokens and returned false, preventing
mounts from starting. Now we keep tokens and return true so the app
can start in offline/degraded mode using cached data."
```

---

### Task 2: Fix `start_mount_common()` to proceed on Network error

**Files:**
- Modify: `crates/carminedesktop-app/src/main.rs:1130-1136`

**Root cause:** The catch-all `Err(e)` branch at line 1130 returns `Ok(None)`, skipping the mount entirely when `check_drive_exists` fails with a Network error.

- [ ] **Step 1: Modify error handling in start_mount_common**

In `crates/carminedesktop-app/src/main.rs`, change lines 1130-1136 from:

```rust
            Err(e) => {
                tracing::warn!(
                    "transient error validating mount '{}': {e}, skipping",
                    mount_config.name
                );
                return Ok(None);
            }
```

To:

```rust
            Err(carminedesktop_core::Error::Network(ref msg)) => {
                tracing::warn!(
                    "mount '{}' offline — network unavailable ({msg}), \
                     proceeding with cached data",
                    mount_config.name
                );
                // Continue to mount creation — VFS will serve from cache.
            }
            Err(e) => {
                tracing::warn!(
                    "transient error validating mount '{}': {e}, skipping",
                    mount_config.name
                );
                return Ok(None);
            }
```

- [ ] **Step 2: Run clippy**

Run: `make clippy`
Expected: No warnings.

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/src/main.rs
git commit -m "fix(app): proceed with mount on network error instead of skipping

When check_drive_exists fails with a Network error, the drive likely
still exists but the network is unavailable. Proceed with mount
creation so the VFS can serve pinned offline content from cache."
```

---

## Chunk 2: Offline Flag Infrastructure

### Task 3: Add offline flag to CoreOps and thread through VFS constructors

**Files:**
- Modify: `crates/carminedesktop-vfs/src/core_ops.rs:468-516` (struct + new + builder)
- Modify: `crates/carminedesktop-vfs/src/fuse_fs.rs:87-126` (CarmineDesktopFs::new)
- Modify: `crates/carminedesktop-vfs/src/mount.rs:94-168` (MountHandle::mount + root fallback)
- Modify: `crates/carminedesktop-vfs/src/winfsp_fs.rs:237+` (CarmineDesktopWinFsp::new)
- Modify: `crates/carminedesktop-vfs/src/winfsp_fs.rs:977+` (WinFspMountHandle::mount + root fallback)

#### 3a: Add offline field to CoreOps

- [ ] **Step 1: Add offline field, helpers, and builder to CoreOps**

In `crates/carminedesktop-vfs/src/core_ops.rs`, add to the `CoreOps` struct (after `inode_invalidator` field):

```rust
    offline: Arc<std::sync::atomic::AtomicBool>,
```

In `CoreOps::new()`, add initialization:

```rust
            offline: Arc::new(std::sync::atomic::AtomicBool::new(false)),
```

Add builder method after `with_inode_invalidator`:

```rust
    pub fn with_offline_flag(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.offline = flag;
        self
    }

    /// Returns `true` when the VFS is operating in offline/cache-only mode.
    fn is_offline(&self) -> bool {
        self.offline.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark the VFS as offline after a network failure.
    fn set_offline(&self) {
        if !self.offline.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(drive_id = %self.drive_id, "VFS entering offline mode — serving from cache only");
        }
    }
```

#### 3b: Thread offline flag through CarmineDesktopFs (FUSE)

- [ ] **Step 2: Add offline_flag param to CarmineDesktopFs::new**

In `crates/carminedesktop-vfs/src/fuse_fs.rs`, add parameter to `CarmineDesktopFs::new()` (line 87-95):

```rust
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        offline_flag: Arc<std::sync::atomic::AtomicBool>,  // NEW
    ) -> Self {
```

And chain it in the CoreOps construction (around line 112):

```rust
        let mut ops = CoreOps::new(graph, cache, inodes, drive_id, rt)
            .with_offline_flag(offline_flag);
```

(Replace the current `let mut ops = CoreOps::new(graph, cache, inodes, drive_id, rt);`)

#### 3c: Thread through MountHandle::mount + add SQLite root fallback

- [ ] **Step 3: Add offline_flag param and root fallback to MountHandle::mount**

In `crates/carminedesktop-vfs/src/mount.rs`, add parameter to `MountHandle::mount()` (line 94-103):

```rust
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        offline_flag: Arc<std::sync::atomic::AtomicBool>,  // NEW
    ) -> carminedesktop_core::Result<Self> {
```

Replace the root item fetch (lines 104-110) with a cache-first approach:

```rust
        // Try to restore root item from SQLite cache first (offline-safe),
        // falling back to Graph API if not cached.
        let root_item = match cache.sqlite.get_item_by_id("root") {
            Ok(Some((_, cached_root))) => {
                tracing::debug!("restored root item from cache for drive {drive_id}");
                // Also try to refresh from network (best-effort)
                match tokio::task::block_in_place(|| {
                    rt.block_on(graph.get_item(&drive_id, "root"))
                }) {
                    Ok(fresh) => fresh,
                    Err(e) => {
                        tracing::warn!("root item refresh failed: {e} — using cached version");
                        offline_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        cached_root
                    }
                }
            }
            _ => {
                // No cache — must fetch from network (first-time mount)
                tokio::task::block_in_place(|| {
                    rt.block_on(graph.get_item(&drive_id, "root"))
                })
                .map_err(|e| {
                    carminedesktop_core::Error::Filesystem(format!(
                        "failed to fetch root item for drive {drive_id}: {e}"
                    ))
                })?
            }
        };
```

**Note:** `get_item_by_id("root")` won't work because the root item's `id` field is the actual Graph ID (e.g., `01ABCDEF...`), not the string `"root"`. We need a different lookup. The root item was stored with `upsert_item(ROOT_INODE, &drive_id, &root_item, None)`, so we can look up by inode. But `SqliteStore` only has `get_item_by_id()` and `get_children()`.

**Better approach:** Query SQLite for inode 1 (ROOT_INODE). Add a thin helper or use a direct query. Actually, let's check if we can get the item by looking up inode 1 in the existing store. Since there's no `get_item_by_inode` method, add one:

In `crates/carminedesktop-cache/src/sqlite.rs`, add after `get_item_by_id`:

```rust
    pub fn get_item_by_inode(
        &self,
        inode: u64,
    ) -> carminedesktop_core::Result<Option<DriveItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare_cached("SELECT json_data FROM items WHERE inode = ?1")
            .map_err(|e| carminedesktop_core::Error::Cache(format!("prepare failed: {e}")))?;

        let result = stmt
            .query_row(params![inode as i64], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .optional()
            .map_err(|e| carminedesktop_core::Error::Cache(format!("query failed: {e}")))?;

        match result {
            Some(json) => {
                let item: DriveItem = serde_json::from_str(&json).map_err(|e| {
                    carminedesktop_core::Error::Cache(format!("deserialize failed: {e}"))
                })?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }
```

Then the root item fallback in `mount.rs` becomes:

```rust
        let root_item = match cache.sqlite.get_item_by_inode(ROOT_INODE) {
            Ok(Some(cached_root)) => {
                tracing::debug!("restored root item from cache for drive {drive_id}");
                match tokio::task::block_in_place(|| {
                    rt.block_on(graph.get_item(&drive_id, "root"))
                }) {
                    Ok(fresh) => fresh,
                    Err(e) => {
                        tracing::warn!("root item refresh failed: {e} — using cached version");
                        offline_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        cached_root
                    }
                }
            }
            _ => {
                tokio::task::block_in_place(|| {
                    rt.block_on(graph.get_item(&drive_id, "root"))
                })
                .map_err(|e| {
                    carminedesktop_core::Error::Filesystem(format!(
                        "failed to fetch root item for drive {drive_id}: {e}"
                    ))
                })?
            }
        };
```

Pass `offline_flag` to `CarmineDesktopFs::new` in the `try_mount` closure (line 155):

```rust
                let fs = CarmineDesktopFs::new(
                    graph.clone(),
                    cache.clone(),
                    inodes.clone(),
                    drive_id.clone(),
                    rt.clone(),
                    event_tx,
                    sync_handle,
                    offline_flag.clone(),  // NEW
                );
```

#### 3d: Same for WinFsp

- [ ] **Step 4: Thread offline_flag through WinFspMountHandle::mount and CarmineDesktopWinFsp::new**

Apply the same changes in `crates/carminedesktop-vfs/src/winfsp_fs.rs`:
- Add `offline_flag: Arc<std::sync::atomic::AtomicBool>` param to `CarmineDesktopWinFsp::new()` (line 237+)
- Chain `.with_offline_flag(offline_flag)` in the CoreOps construction inside `new()`
- Add `offline_flag` param to `WinFspMountHandle::mount()` (line 977+)
- Replace root item fetch with same SQLite-first fallback pattern as mount.rs
- Pass `offline_flag` when constructing `CarmineDesktopWinFsp::new()`

- [ ] **Step 5: Run clippy**

Run: `make clippy`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/carminedesktop-cache/src/sqlite.rs \
        crates/carminedesktop-vfs/src/core_ops.rs \
        crates/carminedesktop-vfs/src/fuse_fs.rs \
        crates/carminedesktop-vfs/src/mount.rs \
        crates/carminedesktop-vfs/src/winfsp_fs.rs
git commit -m "feat(vfs): add offline flag infrastructure with SQLite root fallback

- Add Arc<AtomicBool> offline flag to CoreOps with is_offline/set_offline helpers
- Thread offline_flag through CarmineDesktopFs::new and MountHandle::mount
- Same for WinFsp path
- MountHandle::mount now falls back to SQLite-cached root item when
  Graph API is unavailable, enabling offline mount startup
- Add SqliteStore::get_item_by_inode() for root item recovery"
```

---

### Task 4: Wire offline flag from app to VFS and delta sync

**Files:**
- Modify: `crates/carminedesktop-app/src/main.rs`

**Key structures to modify:**
- `MountContext` (line 1078): add `offline_flag: Arc<AtomicBool>` field
- `MountCacheEntry` (line 88): add 5th element `Arc<AtomicBool>`
- `SyncSnapshotRow` (line 97): add 8th element `Arc<AtomicBool>`
- `start_mount_common` (line 1091): create offline flag
- `start_mount` (FUSE, line 1273): pass to MountHandle::mount + MountCacheEntry
- `start_mount` (Windows, line 1341): same
- `start_delta_sync` (line 1441): use offline flag for set/clear

- [ ] **Step 1: Add offline_flag to MountContext**

In `main.rs`, modify struct `MountContext` (line 1078-1087):

```rust
struct MountContext {
    drive_id: String,
    mountpoint: String,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    offline_manager: Arc<carminedesktop_cache::OfflineManager>,
    offline_flag: Arc<std::sync::atomic::AtomicBool>,  // NEW
    event_tx: tokio::sync::mpsc::UnboundedSender<carminedesktop_vfs::core_ops::VfsEvent>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<carminedesktop_vfs::core_ops::VfsEvent>,
    rt: tokio::runtime::Handle,
}
```

- [ ] **Step 2: Create offline flag in start_mount_common**

In `start_mount_common`, after creating `offline_manager` (around line 1208), add:

```rust
    let offline_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
```

Include it in the returned `MountContext` (around line 1229):

```rust
    Ok(Some(MountContext {
        drive_id: drive_id.to_string(),
        mountpoint,
        cache,
        inodes,
        offline_manager,
        offline_flag,  // NEW
        event_tx,
        event_rx,
        rt,
    }))
```

- [ ] **Step 3: Expand MountCacheEntry to 5-tuple**

Change the type alias (line 88-93):

```rust
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
    Arc<carminedesktop_cache::OfflineManager>,
    Arc<std::sync::atomic::AtomicBool>,  // NEW: offline_flag
);
```

- [ ] **Step 4: Expand SyncSnapshotRow to 8-tuple**

Change the type alias (line 97-105):

```rust
type SyncSnapshotRow = (
    String,
    String,
    String,
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
    Arc<carminedesktop_cache::OfflineManager>,
    Arc<std::sync::atomic::AtomicBool>,  // NEW: offline_flag
);
```

- [ ] **Step 5: Pass offline_flag in start_mount (FUSE)**

In `start_mount` (Linux/macOS, line 1302-1312), add `ctx.offline_flag.clone()` to `MountHandle::mount`:

```rust
    let mut handle = MountHandle::mount(
        state.graph.clone(),
        ctx.cache.clone(),
        ctx.inodes.clone(),
        ctx.drive_id.clone(),
        &ctx.mountpoint,
        ctx.rt.clone(),
        Some(ctx.event_tx),
        Some(sync_handle),
        ctx.offline_flag.clone(),  // NEW
    )
    .map_err(|e| e.to_string())?;
```

Update `mount_caches` insert (line 1319-1322):

```rust
    state.mount_caches.lock().unwrap().insert(
        ctx.drive_id.clone(),
        (ctx.cache, ctx.inodes, observer, ctx.offline_manager, ctx.offline_flag),
    );
```

- [ ] **Step 6: Pass offline_flag in start_mount (Windows)**

Same pattern in the Windows `start_mount` (line 1363-1373 + 1380-1383).

- [ ] **Step 7: Update delta sync snapshot construction**

In `start_delta_sync`, update the snapshot map (line 1472-1491):

```rust
                    .map(|(drive_id, (c, i, obs, offline_mgr, offline_flag))| {
                        // ... existing code ...
                        (
                            drive_id.clone(),
                            mount_id,
                            mount_name,
                            c.clone(),
                            i.clone(),
                            obs.clone(),
                            offline_mgr.clone(),
                            offline_flag.clone(),  // NEW
                        )
                    })
```

Update the loop destructure (line 1494):

```rust
            for (drive_id, mount_id, mount_name, cache, inodes, observer, offline_mgr, offline_flag) in &snapshot {
```

Update the existing `offline` references in the body:
- Line 1512: `let offline = offline_mgr.clone();` (rename the existing offline binding)
- Line 1515: `offline.redownload_changed_items(...)` stays the same (refers to offline_mgr)

**Add:** After `Ok(result)` handler (line 1507), clear offline flag on success:

```rust
                    Ok(result) => {
                        notified_403.remove(drive_id.as_str());
                        // Network is working — exit offline mode
                        offline_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                        // ... existing redownload code ...
                    }
```

**Add:** In the catch-all error handler (line 1554-1556), add Network-specific branch:

```rust
                    Err(carminedesktop_core::Error::Network(_)) => {
                        tracing::warn!("delta sync for {drive_id}: network unavailable");
                        offline_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::error!("delta sync failed for drive {drive_id}: {e}");
                    }
```

- [ ] **Step 8: Update expired-pins loop destructure**

Line 1561:

```rust
            for (_, _, _, _, _, _, offline_mgr, _) in &snapshot {
                if let Err(e) = offline_mgr.process_expired() {
```

- [ ] **Step 9: Update all other MountCacheEntry destructure sites**

Search for all tuple destructures of `mount_caches` values in `main.rs` and update them to include the 5th element. Common patterns:
- `(c, i, obs, offline_mgr)` → `(c, i, obs, offline_mgr, _offline_flag)` (or `_` if unused)

Check lines: ~972, ~1000 (stop_mount, remove_mount), and anywhere else `mount_caches` is accessed.

- [ ] **Step 10: Run clippy**

Run: `make clippy`
Expected: No warnings.

- [ ] **Step 11: Commit**

```bash
git add crates/carminedesktop-app/src/main.rs
git commit -m "feat(app): wire offline flag between CoreOps and delta sync

Create a shared AtomicBool per mount. Pass to VFS MountHandle for
cache-only reads when offline. Delta sync clears the flag on success
and sets it on Network errors. Expanded MountCacheEntry and
SyncSnapshotRow tuples to carry the flag."
```

---

## Chunk 3: VFS Cache-Only Mode

### Task 5: Make VFS operations cache-only when offline

**Files:**
- Modify: `crates/carminedesktop-vfs/src/core_ops.rs` (list_children, find_child, open_file, read_content)
- Create: `crates/carminedesktop-vfs/tests/offline_vfs_tests.rs`

**Design:** When `is_offline()` returns true:
- `list_children`: skip Graph API fallback → return SQLite/memory data (may be empty for non-pinned dirs)
- `find_child`: skip Graph API fallback
- `open_file`: skip `graph.get_item()` metadata refresh; serve disk cache as-is
- `read_content`: skip dirty_inodes check; skip download; return error if no cache

On any Graph API `Network` error, call `set_offline()` so subsequent operations are fast.

- [ ] **Step 1: Write failing test — list_children serves from SQLite when offline**

Create `crates/carminedesktop-vfs/tests/offline_vfs_tests.rs`:

```rust
//! Tests for VFS offline (cache-only) mode.

use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::{DriveItem, FileFacet, FolderFacet, ParentReference};
use carminedesktop_graph::GraphClient;
use carminedesktop_vfs::core_ops::CoreOps;
use carminedesktop_vfs::inode::InodeTable;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use wiremock::MockServer;

const DRIVE_ID: &str = "drive-offline-test";

fn test_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}

fn test_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "carminedesktop-offline-vfs-{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    (cache, base)
}

fn make_file(id: &str, name: &str, size: i64, etag: &str) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size,
        last_modified: None,
        created: None,
        etag: Some(etag.to_string()),
        parent_reference: None,
        folder: None,
        file: Some(FileFacet {
            mime_type: Some("text/plain".to_string()),
            hashes: None,
        }),
        publication: None,
        download_url: None,
        web_url: None,
    }
}

fn make_folder(id: &str, name: &str) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 0,
        last_modified: None,
        created: None,
        etag: None,
        parent_reference: None,
        folder: Some(FolderFacet { child_count: 0 }),
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    }
}

#[tokio::test]
async fn list_children_returns_sqlite_data_when_offline() {
    let server = MockServer::start().await;
    // No mocks registered — any Graph API call would get 404
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("list-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(graph, cache.clone(), inodes.clone(), DRIVE_ID.to_string(), rt)
        .with_offline_flag(offline);

    // Pre-populate SQLite with parent and child
    let parent_ino = inodes.allocate("parent-folder");
    let child = make_file("child-file", "readme.txt", 100, "etag1");
    let child_ino = inodes.allocate(&child.id);
    cache
        .sqlite
        .upsert_item(child_ino, DRIVE_ID, &child, Some(parent_ino))
        .unwrap();

    let children = ops.list_children(parent_ino);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].1.name, "readme.txt");

    // No Graph API calls should have been made
    let requests = server.received_requests().await.unwrap();
    assert!(requests.is_empty(), "offline mode should not make Graph API calls");

    let _ = std::fs::remove_dir_all(&base);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `make test-crate CRATE=carminedesktop-vfs TEST=list_children_returns_sqlite_data_when_offline`
Expected: FAIL — `list_children` ignores offline flag.

- [ ] **Step 3: Write failing test — open_file serves disk cache when offline**

Add to `offline_vfs_tests.rs`:

```rust
#[tokio::test]
async fn open_file_serves_disk_cache_when_offline() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("open-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(graph, cache.clone(), inodes.clone(), DRIVE_ID.to_string(), rt)
        .with_offline_flag(offline);

    // Pre-populate caches: metadata in memory + content in disk cache
    let file = make_file("file1", "doc.txt", 5, "etag-abc");
    let file_ino = inodes.allocate(&file.id);
    cache.memory.insert(file_ino, file.clone());
    cache
        .disk
        .put(DRIVE_ID, &file.id, b"hello", Some("etag-abc"))
        .await
        .unwrap();

    // open_file should succeed from cache without any Graph API calls
    let fh = ops.open_file(file_ino);
    assert!(fh.is_ok(), "open_file should succeed offline with cached content");

    let requests = server.received_requests().await.unwrap();
    assert!(requests.is_empty(), "offline mode should not make Graph API calls");

    let _ = std::fs::remove_dir_all(&base);
}
```

- [ ] **Step 4: Write failing test — read_content serves disk cache when offline**

Add to `offline_vfs_tests.rs`:

```rust
#[tokio::test]
async fn read_content_serves_disk_cache_when_offline() {
    let server = MockServer::start().await;
    let graph = test_graph(&server.uri());
    let (cache, base) = test_cache("read-offline");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(true));

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(graph, cache.clone(), inodes.clone(), DRIVE_ID.to_string(), rt)
        .with_offline_flag(offline);

    let file = make_file("file2", "data.bin", 11, "etag-xyz");
    let file_ino = inodes.allocate(&file.id);
    cache.memory.insert(file_ino, file.clone());
    cache
        .disk
        .put(DRIVE_ID, &file.id, b"hello world", Some("etag-xyz"))
        .await
        .unwrap();

    let content = ops.read_content(file_ino);
    assert!(content.is_ok());
    assert_eq!(content.unwrap(), b"hello world");

    let requests = server.received_requests().await.unwrap();
    assert!(requests.is_empty(), "offline mode should not make Graph API calls");

    let _ = std::fs::remove_dir_all(&base);
}
```

- [ ] **Step 5: Write failing test — network error sets offline flag**

Add to `offline_vfs_tests.rs`:

```rust
#[tokio::test]
async fn network_error_sets_offline_flag() {
    let server = MockServer::start().await;
    let uri = server.uri();
    // Drop the server to simulate network failure
    drop(server);

    let graph = test_graph(&uri);
    let (cache, base) = test_cache("net-error");
    let inodes = Arc::new(InodeTable::new());
    let offline = Arc::new(AtomicBool::new(false));

    let rt = tokio::runtime::Handle::current();
    let ops = CoreOps::new(graph, cache.clone(), inodes.clone(), DRIVE_ID.to_string(), rt)
        .with_offline_flag(offline.clone());

    // Allocate parent but don't populate SQLite — forces Graph API fallback
    let parent_ino = inodes.allocate("empty-parent");

    let _ = ops.list_children(parent_ino);

    // After network failure, offline flag should be set
    assert!(
        offline.load(Ordering::Relaxed),
        "offline flag should be set after network error"
    );

    let _ = std::fs::remove_dir_all(&base);
}
```

- [ ] **Step 6: Run all new tests to verify they fail**

Run: `make test-crate CRATE=carminedesktop-vfs TEST=offline`
Expected: All 4 tests FAIL.

- [ ] **Step 7: Implement offline guard in `list_children`**

In `core_ops.rs`, in `list_children` — after the SQLite check returns empty (line 761), before `let Some(item_id) = ...` (line 763):

```rust
        if self.is_offline() {
            tracing::debug!(parent_ino, "list_children: offline, skipping Graph API fallback");
            return Vec::new();
        }
```

In the Graph API error handler (line 791-794), add `set_offline`:

```rust
            Err(e) => {
                if matches!(&e, carminedesktop_core::Error::Network(_)) {
                    self.set_offline();
                }
                tracing::error!(parent_ino, %item_id, "list_children graph fallback failed: {e}");
                Vec::new()
            }
```

- [ ] **Step 8: Implement offline guard in `find_child`**

In `core_ops.rs`, in `find_child` — after SQLite check (line 698), before `let parent_item_id = ...` (line 700):

```rust
        if self.is_offline() {
            return None;
        }
```

In the Graph API error handler (line 726-728):

```rust
            Err(e) => {
                if matches!(&e, carminedesktop_core::Error::Network(_)) {
                    self.set_offline();
                }
                tracing::warn!(parent_ino, name, "find_child graph fallback failed: {e}");
            }
```

- [ ] **Step 9: Implement offline fast-path in `open_file`**

In `core_ops.rs`, in `open_file` — after the writeback check (line 1008), before the metadata refresh comment (line 1010):

```rust
        // Offline fast-path: skip metadata refresh, serve from disk cache as-is
        if self.is_offline() {
            if let Some((content, _)) = self
                .rt
                .block_on(self.cache.disk.get_with_etag(&self.drive_id, &item_id))
            {
                return Ok(self
                    .open_files
                    .insert(ino, DownloadState::Complete(content)));
            }
            return Err(VfsError::IoError("file not available offline".to_string()));
        }
```

Also add `set_offline()` in the existing `graph.get_item()` error handler (line 1051):

```rust
            Err(e) => {
                if matches!(&e, carminedesktop_core::Error::Network(_)) {
                    self.set_offline();
                }
                tracing::warn!(
                    ino,
                    "open_file: get_item refresh failed: {e}, using cached metadata"
                );
                item
            }
```

- [ ] **Step 10: Implement offline fast-path in `read_content`**

In `core_ops.rs`, in `read_content` — after the writeback check (line 809), before the dirty_inodes check (line 812):

```rust
        // Offline mode: serve from disk cache without freshness validation
        if self.is_offline() {
            if let Some((content, _)) = self
                .rt
                .block_on(self.cache.disk.get_with_etag(&self.drive_id, &item_id))
            {
                return Ok(content);
            }
            return Err(VfsError::IoError("file not available offline".to_string()));
        }
```

Also add `set_offline()` in the download error handler (line 848):

```rust
            Err(e) => {
                if matches!(&e, carminedesktop_core::Error::Network(_)) {
                    self.set_offline();
                }
                tracing::error!("download failed for {item_id}: {e}");
                Err(VfsError::IoError(format!("download failed: {e}")))
            }
```

- [ ] **Step 11: Run all offline VFS tests**

Run: `make test-crate CRATE=carminedesktop-vfs TEST=offline`
Expected: All 4 tests PASS.

- [ ] **Step 12: Run full VFS test suite**

Run: `make test-crate CRATE=carminedesktop-vfs`
Expected: All existing tests still pass.

- [ ] **Step 13: Commit**

```bash
git add crates/carminedesktop-vfs/src/core_ops.rs crates/carminedesktop-vfs/tests/offline_vfs_tests.rs
git commit -m "feat(vfs): cache-only mode when offline flag is set

When the offline AtomicBool is true, VFS operations skip all Graph API
calls and serve exclusively from cache:
- list_children/find_child: memory → SQLite only
- open_file: disk cache without metadata refresh
- read_content: disk cache without freshness validation

Network errors in any VFS operation automatically set the offline flag,
making subsequent operations fast instead of blocking on retries."
```

---

## Chunk 4: Verification

### Task 6: Full build and test verification

- [ ] **Step 1: Run clippy across all targets**

Run: `make clippy`
Expected: Zero warnings.

- [ ] **Step 2: Run full test suite**

Run: `make test`
Expected: All tests pass.

- [ ] **Step 3: Run full build**

Run: `make build`
Expected: Clean build, no errors.

---

## Summary of Changes

| Bug | Root Cause | Fix | File |
|-----|-----------|-----|------|
| Close app → offline → no folders | `try_restore()` clears tokens on refresh failure | Keep tokens, return `Ok(true)` | `manager.rs` |
| Close app → offline → no folders | `start_mount_common()` skips mount on Network error | Proceed with mount on Network error | `main.rs` |
| Close app → offline → no folders | `MountHandle::mount()` fetches root from Graph API | Fall back to SQLite-cached root item | `mount.rs`, `winfsp_fs.rs` |
| Online → offline → explorer freezes | VFS blocks 7-10s on Graph API retries per operation | Offline flag skips all Graph API calls; set on first Network error | `core_ops.rs` |

**Recovery path:** Delta sync loop clears the offline flag when a successful sync completes (~60s after network returns). VFS then resumes normal online operation with metadata refresh and content download.

**New SQLite method:** `get_item_by_inode(u64)` — thin query for root item recovery at mount startup.
