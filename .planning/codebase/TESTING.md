# Testing Patterns

**Analysis Date:** 2026-03-18

## Test Framework

**Runner:**
- Rust built-in test harness (`cargo test`)
- Tokio for async tests: `#[tokio::test]` or `#[tokio::test(flavor = "multi_thread")]`
- No additional test runner (no nextest, no custom harness)

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ne!` macros
- Pattern matching with `matches!()` macro for enum variant checks
- Manual `match` + `panic!` for detailed enum variant assertions with field checks

**HTTP Mocking:**
- `wiremock` 0.6 — workspace dev-dependency
- Config: `Cargo.toml` at workspace root, line 107

**Run Commands:**
```bash
make test                    # Run all tests (via toolbox container)
make check                   # Run fmt-check + clippy + test
cargo test --all-targets     # Direct cargo (inside toolbox)
cargo test -p carminedesktop-vfs --test fuse_integration -- --ignored  # FUSE tests
```

## Test File Organization

**Location:**
- Integration test files in `crates/<name>/tests/` directory (NOT inline `#[cfg(test)]` modules)
- This is a deliberate convention — no `#[cfg(test)] mod tests` blocks exist in source files

**Naming:**
- `crates/<name>/tests/<descriptive_name>.rs`
- Test files grouped by feature/component, not by source module

**Structure:**
```
crates/
├── carminedesktop-core/tests/
│   └── config_tests.rs            # Config parsing, expansion, roundtrip
├── carminedesktop-auth/tests/
│   └── auth_integration.rs        # Token storage, PKCE, sign-in flows
├── carminedesktop-graph/tests/
│   └── graph_tests.rs             # All Graph API client operations
├── carminedesktop-cache/tests/
│   ├── cache_tests.rs             # Memory, SQLite, disk, writeback, pin store
│   └── test_offline.rs            # Offline pinning operations
├── carminedesktop-vfs/tests/
│   ├── fuse_integration.rs        # Real FUSE mount tests (#[ignore])
│   ├── open_file_table_tests.rs   # Open/read/write/flush/release lifecycle
│   ├── sync_processor_tests.rs    # Debounce, concurrency, retry, recovery
│   ├── pending_retry.rs           # Pending write retry after transient failure
│   ├── stale_mount_tests.rs       # Stale FUSE mount cleanup
│   ├── transient_file_tests.rs    # Office lock/temp file detection
│   └── offline_vfs_tests.rs       # VFS offline (cache-only) mode
└── carminedesktop-app/tests/
    └── integration_tests.rs       # E2E flows, initialization, sign-in/out
```

## Test Naming Conventions

**Two patterns coexist:**

1. **Prefixed (cache/auth/core):** `test_<module>_<operation>_<scenario>()`
   - `test_memory_cache_insert_get_roundtrip()`
   - `test_sqlite_store_upsert_get_item_roundtrip()`
   - `test_disk_cache_lru_eviction()`
   - `test_writeback_write_read_roundtrip()`
   - `test_user_config_load_empty()`
   - `test_pin_store_pin_and_is_pinned()`

2. **Descriptive (graph/vfs):** `<operation>_<scenario>()`
   - `get_my_drive_returns_drive()`
   - `list_children_paginates_two_pages()`
   - `error_404_returns_graph_api_error()`
   - `open_returns_unique_handles()`
   - `streaming_buffer_append_updates_progress()`

**Use the pattern matching the crate you're working in.**

## Test Structure

**Suite Organization:**
```rust
// Helper functions at top of file
fn make_client(base_url: &str) -> GraphClient { ... }
fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem { ... }
fn unique_temp_dir(prefix: &str) -> std::path::PathBuf { ... }
fn cleanup(path: &std::path::Path) { ... }

// Section separators for logical grouping
// ============================================================================
// MEMORY CACHE TESTS
// ============================================================================

#[test]  // or #[tokio::test]
fn test_memory_cache_insert_get_roundtrip() {
    // Arrange
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    // Act
    cache.insert(1, item.clone());
    let retrieved = cache.get(1);

    // Assert
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, "item1");
}
```

**Patterns:**
- No explicit Arrange/Act/Assert comments, but the structure is followed
- Helper functions for repeated setup (test data creation, fixture building)
- Section separators (`// ====...`) to group related tests within a file
- Comment separators with descriptive headers: `// --- copy_item tests ---`

## Async Test Patterns

**Simple async tests:**
```rust
#[tokio::test]
async fn get_my_drive_returns_drive() {
    let server = MockServer::start().await;
    // ... mock setup + assertions
}
```

**Tests requiring blocking VFS operations:**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn open_returns_unique_handles() {
    let server = MockServer::start().await;
    // ... setup ...
    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        // ... sync assertions inside spawn_blocking ...
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();
    cleanup(&base);
}
```

**Use `flavor = "multi_thread"` when:**
- Test code calls `rt.block_on()` internally (VFS `CoreOps` methods)
- FUSE integration tests that spawn filesystem threads
- Sync processor tests with concurrent uploads

**Time-sensitive retry tests:**
```rust
#[tokio::test]
async fn error_429_retries_then_fails() {
    tokio::time::pause();  // Freeze time for deterministic retry testing
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .expect(4)  // 1 initial + 3 retries
        .mount(&server)
        .await;
    // ... test proceeds with paused time ...
}
```

## Mocking

**Framework:** `wiremock` 0.6

**Standard Mock Pattern:**
```rust
#[tokio::test]
async fn download_content_returns_bytes() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/i1/content"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(payload.to_vec(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let client = GraphClient::with_base_url(server.uri(), || async {
        Ok("test-token".to_string())
    });
    let data = client.download_content("d1", "i1").await.unwrap();
    assert_eq!(data.as_ref(), payload);
}
```

**Mock Construction Helpers:**
```rust
// Reusable JSON builder for DriveItem responses
fn drive_item_json(id: &str, name: &str, size: i64) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "size": size,
        "webUrl": format!("https://contoso.sharepoint.com/Shared%20Documents/{name}"),
    })
}
```

**GraphClient Test Construction:**
```rust
// Always use with_base_url pointing to MockServer URI
fn make_client(base_url: &str) -> GraphClient {
    GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    })
}

// Or as Arc for shared ownership in VFS tests
fn make_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}
```

**What to Mock:**
- All Microsoft Graph API HTTP calls via `wiremock::MockServer`
- Token acquisition (replaced by static `"test-token"` closure)
- Responses including pagination (`@odata.nextLink`), error bodies, status codes

**What NOT to Mock:**
- SQLite database (use real in-memory or temp-file databases)
- Disk cache (use real filesystem with temp directories)
- Memory cache (use real `DashMap`-backed `MemoryCache`)
- Writeback buffer (use real filesystem)
- `CacheManager` (always instantiate real instances with temp paths)

**Expectation Verification:**
```rust
// Verify exact number of requests
Mock::given(method("GET"))
    .respond_with(...)
    .expect(4)       // Exactly 4 calls expected
    .mount(&server)
    .await;

// Verify specific request properties
let requests = server.received_requests().await.unwrap();
let poll_req = requests.iter().find(|r| r.url.path() == "/monitor/abc").unwrap();
assert!(!poll_req.headers.iter().any(|(name, _)| name == "authorization"));

// Scoped mocks for single-call verification
let scoped_mock = Mock::given(...)
    .expect(1)
    .mount_as_scoped(&server)
    .await;
// ... test code ...
drop(scoped_mock);  // Panics if call count doesn't match
```

## Fixtures and Factories

**Test Data (`DriveItem` factory):**
```rust
fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem {
    DriveItem {
        id: id.to_string(),
        name: name.to_string(),
        size: 1024,
        last_modified: None,
        created: None,
        etag: Some(format!("etag-{}", id)),
        parent_reference: None,
        folder: if is_folder {
            Some(FolderFacet { child_count: 0 })
        } else {
            None
        },
        file: None,
        publication: None,
        download_url: None,
        web_url: None,
    }
}
```

**Temp Directory Pattern:**
```rust
fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("carminedesktop-{prefix}-{id}"))
}

// In cache tests, use std::env::temp_dir() with descriptive join
let db_path = std::env::temp_dir().join("test_sqlite_open.db");
let _ = std::fs::remove_file(&db_path);  // Explicit cleanup BEFORE test
```

**CacheManager Factory:**
```rust
fn make_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = unique_test_dir(prefix);
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    (cache, base)
}
```

**Location:**
- Test helpers are defined at the top of each test file (no shared test utility crate)
- Each test file defines its own helpers suited to its needs
- Common pattern: `make_client()`, `make_cache()`, `make_graph()`, `test_drive_item()`, `cleanup()`

## File I/O in Tests

**Critical pattern: explicit cleanup BEFORE each test:**
```rust
#[test]
fn test_sqlite_store_open() -> carminedesktop_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_open.db");
    let _ = std::fs::remove_file(&db_path);   // ← Clean up BEFORE test
    let _store = SqliteStore::open(&db_path)?;
    assert!(db_path.exists());
    Ok(())
}
```

**Temp directory cleanup pattern:**
```rust
fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
}

// Used at end of tests
cleanup(&base);

// Or for tests that create directories
let _ = std::fs::remove_dir_all(&cache_dir);
std::fs::create_dir_all(&cache_dir)?;
```

## Coverage

**Requirements:** No formal coverage target enforced.

**View Coverage:**
```bash
# Not configured — no coverage tool in Makefile or CI
```

## Test Types

**Unit Tests (synchronous `#[test]`):**
- Memory cache operations: `crates/carminedesktop-cache/tests/cache_tests.rs`
- SQLite store operations: `crates/carminedesktop-cache/tests/cache_tests.rs`
- Config parsing and validation: `crates/carminedesktop-core/tests/config_tests.rs`
- Token serialization: `crates/carminedesktop-auth/tests/auth_integration.rs`
- Transient file detection: `crates/carminedesktop-vfs/tests/transient_file_tests.rs`
- Conflict naming: `crates/carminedesktop-vfs/tests/open_file_table_tests.rs`
- Stale mount cleanup: `crates/carminedesktop-vfs/tests/stale_mount_tests.rs`
- Pin store CRUD: `crates/carminedesktop-cache/tests/cache_tests.rs`

**Integration Tests (async `#[tokio::test]`):**
- Graph API client with mocked HTTP: `crates/carminedesktop-graph/tests/graph_tests.rs`
- Disk cache with real filesystem: `crates/carminedesktop-cache/tests/cache_tests.rs`
- Writeback buffer crash recovery: `crates/carminedesktop-cache/tests/cache_tests.rs`
- Delta sync with mock server: `crates/carminedesktop-cache/tests/cache_tests.rs`
- VFS open/read/write lifecycle: `crates/carminedesktop-vfs/tests/open_file_table_tests.rs`
- Sync processor debounce/concurrency: `crates/carminedesktop-vfs/tests/sync_processor_tests.rs`
- Pending write retry: `crates/carminedesktop-vfs/tests/pending_retry.rs`
- Offline mode: `crates/carminedesktop-vfs/tests/offline_vfs_tests.rs`
- App-level initialization/sign-in/shutdown: `crates/carminedesktop-app/tests/integration_tests.rs`

**FUSE/WinFsp E2E Tests (`#[ignore]`):**
- Real FUSE mount tests: `crates/carminedesktop-vfs/tests/fuse_integration.rs`
  - Requires FUSE installed, marked `#[ignore = "requires FUSE"]`
  - Run explicitly: `cargo test -p carminedesktop-vfs --test fuse_integration -- --ignored`
  - Use `#[tokio::test(flavor = "multi_thread")]`
  - Use `TestFixture` struct with setup/teardown lifecycle
- macOS smoke test: `crates/carminedesktop-app/tests/integration_tests.rs` (`#[ignore = "requires macOS with macFUSE installed"]`)
- Windows WinFsp smoke test: `crates/carminedesktop-app/tests/integration_tests.rs` (`#[ignore = "requires Windows with WinFsp"]`)
- Live Graph API tests: `crates/carminedesktop-app/tests/integration_tests.rs` (`#[ignore = "requires live Graph API"]`)

## Common Patterns

**Error Variant Assertion:**
```rust
// Pattern 1: match + panic for detailed field checks
let err = client.get_my_drive().await.unwrap_err();
match err {
    carminedesktop_core::Error::GraphApi { status, message } => {
        assert_eq!(status, 404);
        assert!(message.contains("itemNotFound"));
    }
    other => panic!("expected GraphApi error, got: {other:?}"),
}

// Pattern 2: matches! for simple variant checks
assert!(matches!(err, carminedesktop_core::Error::Locked));
assert!(matches!(result, PinResult::Rejected { .. }));
```

**Result-returning Tests:**
```rust
#[test]
fn test_sqlite_store_open() -> carminedesktop_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_open.db");
    let _ = std::fs::remove_file(&db_path);
    let _store = SqliteStore::open(&db_path)?;   // ← ? propagation
    assert!(db_path.exists());
    Ok(())
}

#[tokio::test]
async fn test_disk_cache_put_get_roundtrip() -> carminedesktop_core::Result<()> {
    // ... async test with ? propagation ...
    Ok(())
}
```

**Platform-gated Tests:**
```rust
#[test]
#[cfg(not(target_os = "windows"))]
fn test_expand_mount_point_no_placeholder() { ... }

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FUSE"]
async fn mount_and_unmount_lifecycle() { ... }

// File-level platform gate for entire test module
#![cfg(any(target_os = "linux", target_os = "macos"))]
```

**FUSE Integration Test Fixture:**
```rust
struct TestFixture {
    mount: Option<MountHandle>,
    mountpoint: PathBuf,
    _base_dir: PathBuf,
}

impl TestFixture {
    async fn setup(server: &MockServer) -> Self {
        // Create temp dirs, graph client, cache, inodes
        // Mount FUSE filesystem
        // Wait for mount to initialize
        Self { mount: Some(mount), mountpoint, _base_dir: base }
    }
    fn teardown(mut self) {
        if let Some(m) = self.mount.take() { let _ = m.unmount(); }
        let _ = std::fs::remove_dir_all(&self._base_dir);
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        if let Some(m) = self.mount.take() { let _ = m.unmount(); }
    }
}
```

**Mock Delta Sync Observer (trait mock):**
```rust
struct MockDeltaSyncObserver {
    last_ino: AtomicU64,
    call_count: AtomicU64,
}

impl carminedesktop_core::DeltaSyncObserver for MockDeltaSyncObserver {
    fn on_inode_content_changed(&self, ino: u64) {
        self.last_ino.store(ino, Ordering::Relaxed);
        self.call_count.fetch_add(1, Ordering::Relaxed);
    }
}
```

## Key Testing Conventions

1. **Never use `#[cfg(test)] mod tests`** — all tests go in `crates/<name>/tests/`
2. **Always clean up temp files before each test**, not after (prevents stale state from failed runs)
3. **Use `wiremock` for all HTTP mocking** — never make real network calls in non-ignored tests
4. **Use real filesystem and databases** — no mocking of SQLite, disk cache, or writeback
5. **Return `carminedesktop_core::Result<()>`** from tests that use `?` for error propagation
6. **Use `tokio::time::pause()`** for deterministic retry/backoff testing
7. **Use `tokio::task::spawn_blocking`** when testing sync code that internally calls `rt.block_on()`
8. **Mark platform-dependent tests with `#[ignore]`** and describe the requirement in the ignore reason
9. **Use `.expect(N)` on mocks** to verify exact call counts for retry and deduplication tests
10. **Test helpers are file-local** — each test file defines its own factories and setup functions

---

*Testing analysis: 2026-03-18*
