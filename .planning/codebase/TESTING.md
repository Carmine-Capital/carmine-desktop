# Testing Patterns

**Analysis Date:** 2026-03-10

## Test Framework

**Runner:**
- `cargo test` (Rust built-in test harness)
- Async runtime: `tokio` with `#[tokio::test]` macro
- Test flavor options: `#[tokio::test(flavor = "multi_thread")]` for concurrent tests

**Assertion Library:**
- Standard Rust: `assert!()`, `assert_eq!()`, `assert_ne!()`
- No external assertion libraries required

**Run Commands:**
```bash
make test                       # Run all tests (cargo test --all-targets)
cargo test --all-targets       # Manual: run all integration and unit tests
cargo test --lib               # Run library tests only
cargo test -- --nocapture      # Show output from tests (println!, tracing)
```

**CI Integration:**
- Makefile target: `make check` runs all CI validation (fmt + clippy + test)
- CI enforces: `RUSTFLAGS=-Dwarnings` during clippy runs (no warnings tolerated)

## Test File Organization

**Location:**
- Integration tests: `crates/<crate>/tests/*.rs` (separate from source)
- NOT inline `#[cfg(test)]` modules — integration test convention enforced
- Example paths:
  - `crates/cloudmount-cache/tests/cache_tests.rs`
  - `crates/cloudmount-graph/tests/graph_tests.rs`
  - `crates/cloudmount-vfs/tests/open_file_table_tests.rs`

**Naming:**
- Test files: `<module>_tests.rs` suffix (e.g., `cache_tests.rs`)
- Test functions:
  - Format: `test_<module>_<operation>_<scenario>()` for cache/auth
  - Format: `<operation>_<scenario>()` for graph/vfs (shorter, omit module prefix)
  - All lowercase, underscores separate concepts
  - Examples: `test_memory_cache_insert_get_roundtrip()`, `list_children_paginates_two_pages()`, `open_returns_unique_handles()`

**File Structure:**
```
crates/cloudmount-cache/
├── src/
│   ├── lib.rs
│   ├── memory.rs
│   ├── sqlite.rs
│   └── ...
└── tests/
    └── cache_tests.rs          # All cache tests in one file
```

## Test Structure

**Sync Tests (standard):**
```rust
#[test]
fn test_memory_cache_insert_get_roundtrip() {
    let cache = MemoryCache::new(Some(60));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item.clone());
    let retrieved = cache.get(1);

    assert!(retrieved.is_some());
    let retrieved_item = retrieved.unwrap();
    assert_eq!(retrieved_item.id, "item1");
}
```

**Async Tests (with Tokio):**
```rust
#[tokio::test]
async fn get_my_drive_returns_drive() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "drive-123",
            "name": "OneDrive",
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let drive = client.get_my_drive().await.unwrap();

    assert_eq!(drive.id, "drive-123");
}
```

**Async Tests with Blocking (VFS):**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn read_slices_from_buffer() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("read-slice");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, world!");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}
```

**Pattern for Fallible Tests (return Result):**
```rust
#[test]
fn test_sqlite_store_upsert_get_item_roundtrip() -> cloudmount_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_upsert.db");
    let _ = std::fs::remove_file(&db_path);

    let store = SqliteStore::open(&db_path)?;
    let item = test_drive_item("item1", "test.txt", false);

    store.upsert_item(1, "drive1", &item, None)?;
    let retrieved = store.get_item_by_id("item1")?;

    assert!(retrieved.is_some());
    Ok(())
}
```

## Setup and Teardown

**Helper Functions:**
- Defined in test files before test functions (private by convention)
- Naming: descriptive verb or `setup_`, `make_`, `unique_` prefix
- Examples:
  - `fn test_drive_item(id: &str, name: &str, is_folder: bool) -> DriveItem` — factory for test data
  - `fn make_client(base_url: &str) -> GraphClient` — construct client with test config
  - `fn make_cache(prefix: &str) -> (Arc<CacheManager>, PathBuf)` — setup cache with unique temp dir
  - `fn unique_cache_dir(prefix: &str) -> PathBuf` — generate unique temp directory
  - `fn setup_core_ops(...) -> Arc<CoreOps>` — complex multi-step initialization
  - `fn cleanup(path: &Path)` — explicit file/directory cleanup

**Cleanup Patterns:**
- File I/O tests use `std::env::temp_dir()` with explicit cleanup before each test
- Cleanup call at test end: `cleanup(&base)` or `std::fs::remove_dir_all(&path)`
- Cleanup before test initialization: remove existing temp file/dir to ensure clean state
```rust
let db_path = std::env::temp_dir().join("test_sqlite_open.db");
let _ = std::fs::remove_file(&db_path);  // Clean before test
let _store = SqliteStore::open(&db_path)?;
```

**Time-based Tests:**
- Use `tokio::time::pause()` for deterministic time handling in async tests
- Avoid `std::thread::sleep()` in async contexts — use `tokio::time::sleep()` instead
- Example with sleep:
```rust
#[test]
fn test_memory_cache_ttl_expiry() {
    let cache = MemoryCache::new(Some(1));
    let item = test_drive_item("item1", "test.txt", false);

    cache.insert(1, item);
    assert!(cache.get(1).is_some());

    std::thread::sleep(Duration::from_secs(2));
    assert!(cache.get(1).is_none());
}
```

## Mocking

**Framework:** `wiremock` (HTTP mocking for Graph API tests)

**Pattern:**
```rust
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn example() {
    let server = MockServer::start().await;

    // Mount mock with matchers
    Mock::given(method("GET"))
        .and(path("/me/drive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "drive-123",
        })))
        .mount(&server)
        .await;

    // Use mocked server
    let client = GraphClient::with_base_url(server.uri(), || async {
        Ok("test-token".to_string())
    });
    let result = client.get_my_drive().await;
}
```

**Matchers Commonly Used:**
- `method("GET")`, `method("POST")`, `method("PUT")`, `method("DELETE")`
- `path("/endpoint")` or `path(format!("/drives/{id}/items/{item_id}"))` — exact path matching
- `header("Authorization", "Bearer test-token")` — header assertions
- `header_exists("header-name")` — existence check without value matching

**Response Templates:**
- `.set_body_json(json!({...}))` — JSON body from `serde_json::json!` macro
- `.set_body_raw(bytes, "content-type")` — binary/raw content
- `.expect(1)` — assert exactly N calls (optional, for strict validation)

**What to Mock:**
- External HTTP calls (Graph API, authentication endpoints)
- Downstream services integration tests rely on
- All network I/O in unit-style integration tests

**What NOT to Mock:**
- Cache behavior — test actual `CacheManager`, `MemoryCache`, etc.
- Filesystem operations on small temp directories
- Internal VFS logic (only mock Graph API calls)
- Serialization/deserialization (test actual serde roundtrips)

## Fixtures and Test Data

**Factory Functions:**
- Create test objects with sensible defaults
- Located in test file before first test function (convention)
- Pattern from `crates/cloudmount-cache/tests/cache_tests.rs`:
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

**Helper Builders:**
- For complex setups, use cascading builders or setup functions
- Example from `crates/cloudmount-vfs/tests/open_file_table_tests.rs`:
```rust
fn setup_core_ops(graph: Arc<GraphClient>, cache: Arc<CacheManager>) -> Arc<CoreOps> {
    let inodes = Arc::new(InodeTable::new());
    let rt = tokio::runtime::Handle::current();

    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(1, DriveItem { /* ... */ });

    Arc::new(CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt))
}
```

## Coverage

**Requirements:**
- No explicit coverage target enforced
- CI runs: `cargo test --all-targets` (all tests must pass)
- Test density is high: cache layer, VFS, graph client all have comprehensive tests

**View Coverage:**
- Use `cargo tarpaulin` for coverage reports (not configured by default)
- Coverage analysis is ad-hoc when requested, not enforced by CI

## Test Types & Scope

**Unit-Style Integration Tests (most common):**
- Single test file per crate's major component
- Test file: `crates/cloudmount-cache/tests/cache_tests.rs`
- Scope: CacheManager behavior including all three tiers (memory → SQLite → disk)
- Mocking: minimal (only external APIs like Graph)
- Example: `test_memory_cache_insert_get_roundtrip()`

**API Client Tests:**
- File: `crates/cloudmount-graph/tests/graph_tests.rs`
- Scope: GraphClient methods with wiremock HTTP mocking
- Each test: one Graph API call scenario (success, pagination, error handling)
- Pattern: mount mock → create client → make call → assert response

**VFS Core Logic Tests:**
- File: `crates/cloudmount-vfs/tests/open_file_table_tests.rs`, `stale_mount_tests.rs`
- Scope: File handle lifecycle, cache interaction, concurrent access
- Setup: wiremock server + real CacheManager + real InodeTable
- Pattern: spawn blocking task → perform VFS operation → assert internal state changes

**Configuration & Serialization Tests:**
- File: `crates/cloudmount-core/tests/config_tests.rs`
- Scope: TOML parsing, config merging, path expansion
- Mocking: none (filesystem I/O with temp directory)

## Common Test Patterns

**Async HTTP Test (Graph API):**
```rust
#[tokio::test]
async fn list_children_paginates_two_pages() {
    let server = MockServer::start().await;
    let page2_url = format!("{}/page2", server.uri());

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/root/children"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [drive_item_json("item1", "file1.txt", 100)],
            "@odata.nextLink": page2_url,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [drive_item_json("item2", "file2.txt", 200)],
        })))
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let items = client.list_children("d1", "root").await.unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "item1");
}
```

**Sync Cache Test:**
```rust
#[test]
fn test_memory_cache_insert_with_children() {
    let cache = MemoryCache::new(Some(60));
    let folder = test_drive_item("folder1", "my_folder", true);
    let children: HashMap<String, u64> = HashMap::from([
        ("a.txt".into(), 10),
        ("b.txt".into(), 11),
    ]);

    cache.insert_with_children(1, folder, children.clone());
    let retrieved_children = cache.get_children(1);

    assert!(retrieved_children.is_some());
    assert_eq!(retrieved_children.unwrap(), children);
}
```

**Async Blocking Task (VFS):**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn write_mutates_buffer_in_place() {
    let server = MockServer::start().await;
    mock_file_download(&server, b"Hello, world!").await;

    let (cache, base) = make_cache("write-inplace");
    let graph = make_graph(&server.uri());
    let ops = setup_core_ops(graph, cache);

    let ops2 = ops.clone();
    tokio::task::spawn_blocking(move || {
        let fh = ops2.open_file(2).unwrap();
        let written = ops2.write_handle(fh, 7, b"Rust!").unwrap();
        assert_eq!(written, 5);
        let data = ops2.read_handle(fh, 0, 100).unwrap();
        assert_eq!(data, b"Hello, Rust!!");
        let _ = ops2.release_file(fh);
    })
    .await
    .unwrap();

    cleanup(&base);
}
```

**Fallible Test (returns Result):**
```rust
#[test]
fn test_sqlite_store_open() -> cloudmount_core::Result<()> {
    let db_path = std::env::temp_dir().join("test_sqlite_open.db");
    let _ = std::fs::remove_file(&db_path);

    let _store = SqliteStore::open(&db_path)?;
    assert!(db_path.exists());

    Ok(())
}
```

**Streaming/Iteration Test:**
```rust
#[tokio::test]
async fn download_streaming_yields_chunks() {
    let server = MockServer::start().await;
    let payload: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();

    Mock::given(method("GET"))
        .and(path("/drives/d1/items/i1/content"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(payload.clone(), "application/octet-stream"),
        )
        .mount(&server)
        .await;

    let client = make_client(&server.uri());
    let mut stream = client.download_streaming("d1", "i1").await.unwrap();

    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        collected.extend_from_slice(&chunk);
    }

    assert_eq!(collected, payload);
}
```

## Test Execution & Debugging

**Run Specific Test:**
```bash
cargo test --all-targets test_name -- --nocapture
```

**Run Single Test File:**
```bash
cargo test --test cache_tests -- --nocapture
```

**Show Output (println/tracing):**
```bash
cargo test -- --nocapture --test-threads=1
```

**Debug with Logging:**
- Enable `tracing` output in tests by initializing subscriber
- Or run with environment filter: `RUST_LOG=debug cargo test`

---

*Testing analysis: 2026-03-10*
