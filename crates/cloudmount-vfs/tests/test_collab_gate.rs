use std::sync::Arc;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use cloudmount_cache::CacheManager;
use cloudmount_core::config::CollaborativeOpenConfig;
use cloudmount_core::types::{CollabOpenResponse, DriveItem, FileFacet, ParentReference};
use cloudmount_graph::GraphClient;
use cloudmount_vfs::core_ops::{CoreOps, VfsEvent};
use cloudmount_vfs::inode::InodeTable;

const DRIVE_ID: &str = "test-drive";
const ROOT_ITEM_ID: &str = "root-id";
const FILE_ITEM_ID: &str = "collab-file-1";

fn make_graph(base_url: &str) -> Arc<GraphClient> {
    Arc::new(GraphClient::with_base_url(base_url.to_string(), || async {
        Ok("test-token".to_string())
    }))
}

fn unique_cache_dir(prefix: &str) -> std::path::PathBuf {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cloudmount-collab-{prefix}-{id}"))
}

fn make_cache(prefix: &str) -> (Arc<CacheManager>, std::path::PathBuf) {
    let base = unique_cache_dir(prefix);
    let cache_dir = base.join("cache");
    let db_path = base.join("metadata.db");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache = Arc::new(CacheManager::new(cache_dir, db_path, 100_000_000, Some(300)).unwrap());
    (cache, base)
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p);
}

/// Build a DriveItem representing a collaborative file with the given name.
fn make_file_item(name: &str) -> DriveItem {
    DriveItem {
        id: FILE_ITEM_ID.to_string(),
        name: name.to_string(),
        size: 100,
        last_modified: None,
        created: None,
        etag: Some("etag-collab".to_string()),
        parent_reference: Some(ParentReference {
            drive_id: Some(DRIVE_ID.to_string()),
            id: Some(ROOT_ITEM_ID.to_string()),
            path: None,
        }),
        folder: None,
        file: Some(FileFacet {
            mime_type: None,
            hashes: None,
        }),
        publication: None,
        download_url: None,
        web_url: Some("https://example.sharepoint.com/test.docx".to_string()),
    }
}

/// Build a CollaborativeOpenConfig that treats the current test process as an
/// interactive shell by adding its resolved process name to `shell_processes`.
fn collab_config_for_test_process() -> CollaborativeOpenConfig {
    let mut extra = Vec::new();
    if let Some(name) = cloudmount_vfs::process_filter::current_process_name() {
        extra.push(name);
    }
    CollaborativeOpenConfig {
        timeout_seconds: 15,
        shell_processes: extra,
    }
}

/// Set up inodes and memory cache with root + one file item.
fn setup_inodes_and_cache(cache: &Arc<CacheManager>, item: &DriveItem) -> Arc<InodeTable> {
    let inodes = Arc::new(InodeTable::new());
    inodes.set_root(ROOT_ITEM_ID);
    cache.memory.insert(
        1,
        DriveItem {
            id: ROOT_ITEM_ID.to_string(),
            name: "root".to_string(),
            size: 0,
            last_modified: None,
            created: None,
            etag: None,
            parent_reference: None,
            folder: Some(cloudmount_core::types::FolderFacet { child_count: 0 }),
            file: None,
            publication: None,
            download_url: None,
            web_url: None,
        },
    );

    let file_ino = inodes.allocate(FILE_ITEM_ID);
    cache.memory.insert(file_ino, item.clone());
    inodes
}

/// Mock the Graph API `get_item` endpoint for our test file.
async fn mock_get_item(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": FILE_ITEM_ID,
            "name": "test.docx",
            "size": 100,
            "eTag": "etag-collab",
            "parentReference": { "driveId": DRIVE_ID, "id": ROOT_ITEM_ID },
            "file": { "mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document" },
            "webUrl": "https://example.sharepoint.com/test.docx"
        })))
        .mount(server)
        .await;
}

/// Mock the file download endpoint.
async fn mock_file_download(server: &MockServer, content: &[u8]) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/drives/{DRIVE_ID}/items/{FILE_ITEM_ID}/content"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(content.to_vec(), "application/octet-stream"),
        )
        .mount(server)
        .await;
}

// ---------------------------------------------------------------------------
// CollabGate sends request for collaborative file opened by shell
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn collab_gate_sends_request_for_collaborative_file() {
    let server = MockServer::start().await;
    mock_get_item(&server).await;
    mock_file_download(&server, b"word document content placeholder").await;

    let (cache, base) = make_cache("collab-sends-request");
    let graph = make_graph(&server.uri());
    let item = make_file_item("test.docx");
    let inodes = setup_inodes_and_cache(&cache, &item);
    let file_ino = 2u64; // first allocated inode after root (1)

    let (collab_tx, mut collab_rx) = tokio::sync::mpsc::channel(1);
    let config = collab_config_for_test_process();
    let rt = tokio::runtime::Handle::current();

    let ops = Arc::new(
        CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt)
            .with_collab_sender(collab_tx)
            .with_collab_config(config),
    );

    // Spawn a responder that records the request and replies OpenLocally.
    let (req_tx, req_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        if let Some((request, reply)) = collab_rx.recv().await {
            let _ = req_tx.send(request);
            let _ = reply.send(CollabOpenResponse::OpenLocally);
        }
    });

    let pid = std::process::id();
    let ops2 = ops.clone();
    let result =
        tokio::task::spawn_blocking(move || ops2.open_file(file_ino, Some(pid), Some("test.docx")))
            .await
            .unwrap();

    // Verify the request was received.
    let request = req_rx.await.expect("collab request should have been sent");
    assert_eq!(request.path, "test.docx");
    assert_eq!(request.extension, ".docx");
    assert_eq!(request.item_id, FILE_ITEM_ID);
    assert_eq!(
        request.web_url.as_deref(),
        Some("https://example.sharepoint.com/test.docx")
    );
    // OpenLocally response should let open_file succeed.
    assert!(result.is_ok(), "open_file should succeed with OpenLocally");

    cleanup(&base);
}

// ---------------------------------------------------------------------------
// CollabGate skips non-collaborative files
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn collab_gate_skips_non_collaborative_file() {
    let server = MockServer::start().await;
    mock_get_item(&server).await;
    mock_file_download(&server, b"pdf content placeholder").await;

    let (cache, base) = make_cache("collab-skip-non-collab");
    let graph = make_graph(&server.uri());
    let item = make_file_item("report.pdf");
    let inodes = setup_inodes_and_cache(&cache, &item);
    let file_ino = 2u64;

    let (collab_tx, mut collab_rx) = tokio::sync::mpsc::channel(1);
    let config = collab_config_for_test_process();
    let rt = tokio::runtime::Handle::current();

    let ops = Arc::new(
        CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt)
            .with_collab_sender(collab_tx)
            .with_collab_config(config),
    );

    let pid = std::process::id();
    let ops2 = ops.clone();
    let result = tokio::task::spawn_blocking(move || {
        ops2.open_file(file_ino, Some(pid), Some("report.pdf"))
    })
    .await
    .unwrap();

    // open_file should proceed without triggering the collab channel.
    assert!(
        result.is_ok(),
        "open_file should succeed for non-collaborative file"
    );

    // Verify the channel is empty — no request was sent.
    assert!(
        collab_rx.try_recv().is_err(),
        "no collab request should be sent for .pdf files"
    );

    cleanup(&base);
}

// ---------------------------------------------------------------------------
// CollabGate skips non-interactive processes
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn collab_gate_skips_non_interactive_process() {
    let server = MockServer::start().await;
    mock_get_item(&server).await;
    mock_file_download(&server, b"word document content placeholder").await;

    let (cache, base) = make_cache("collab-skip-non-interactive");
    let graph = make_graph(&server.uri());
    let item = make_file_item("test.docx");
    let inodes = setup_inodes_and_cache(&cache, &item);
    let file_ino = 2u64;

    let (collab_tx, mut collab_rx) = tokio::sync::mpsc::channel(1);
    // Do NOT add the test process name to extra_shells — it will not be
    // recognized as an interactive shell.
    let config = CollaborativeOpenConfig {
        timeout_seconds: 15,
        shell_processes: Vec::new(),
    };
    let rt = tokio::runtime::Handle::current();

    let ops = Arc::new(
        CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt)
            .with_collab_sender(collab_tx)
            .with_collab_config(config),
    );

    let pid = std::process::id();
    let ops2 = ops.clone();
    let result =
        tokio::task::spawn_blocking(move || ops2.open_file(file_ino, Some(pid), Some("test.docx")))
            .await
            .unwrap();

    // open_file should proceed without triggering the collab channel.
    assert!(
        result.is_ok(),
        "open_file should succeed for non-interactive process"
    );

    // Verify no request was sent.
    assert!(
        collab_rx.try_recv().is_err(),
        "no collab request should be sent for non-interactive process"
    );

    cleanup(&base);
}

// ---------------------------------------------------------------------------
// CollabGate timeout falls back to local open
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn collab_gate_timeout_falls_back_to_local() {
    let server = MockServer::start().await;
    mock_get_item(&server).await;
    mock_file_download(&server, b"word document content placeholder").await;

    let (cache, base) = make_cache("collab-timeout");
    let graph = make_graph(&server.uri());
    let item = make_file_item("test.docx");
    let inodes = setup_inodes_and_cache(&cache, &item);
    let file_ino = 2u64;

    let (collab_tx, mut collab_rx) = tokio::sync::mpsc::channel(1);
    let mut config = collab_config_for_test_process();
    config.timeout_seconds = 1; // very short timeout

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let rt = tokio::runtime::Handle::current();

    let ops = Arc::new(
        CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt)
            .with_collab_sender(collab_tx)
            .with_collab_config(config)
            .with_event_sender(event_tx),
    );

    // Spawn a receiver that accepts the request but holds the reply sender
    // without responding, so the timeout fires (not the channel-closed path).
    tokio::spawn(async move {
        if let Some((_request, reply)) = collab_rx.recv().await {
            // Sleep longer than the 1-second timeout while keeping the sender alive.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            drop(reply);
        }
    });

    let pid = std::process::id();
    let ops2 = ops.clone();
    let result =
        tokio::task::spawn_blocking(move || ops2.open_file(file_ino, Some(pid), Some("test.docx")))
            .await
            .unwrap();

    // The open should succeed (fell back to local).
    assert!(
        result.is_ok(),
        "open_file should fall back to local on timeout"
    );

    // Verify a CollabGateTimeout event was emitted.
    let mut found_timeout = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, VfsEvent::CollabGateTimeout { .. }) {
            found_timeout = true;
            break;
        }
    }
    assert!(
        found_timeout,
        "CollabGateTimeout event should have been emitted"
    );

    cleanup(&base);
}

// ---------------------------------------------------------------------------
// OpenOnline response returns CollabRedirect error
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn collab_gate_open_online_returns_redirect_error() {
    let server = MockServer::start().await;
    // No need for get_item / download mocks — the request should be
    // intercepted before they are called.

    let (cache, base) = make_cache("collab-open-online");
    let graph = make_graph(&server.uri());
    let item = make_file_item("test.docx");
    let inodes = setup_inodes_and_cache(&cache, &item);
    let file_ino = 2u64;

    let (collab_tx, mut collab_rx) = tokio::sync::mpsc::channel(1);
    let config = collab_config_for_test_process();
    let rt = tokio::runtime::Handle::current();

    let ops = Arc::new(
        CoreOps::new(graph, cache, inodes, DRIVE_ID.to_string(), rt)
            .with_collab_sender(collab_tx)
            .with_collab_config(config),
    );

    // Respond with OpenOnline.
    tokio::spawn(async move {
        if let Some((_request, reply)) = collab_rx.recv().await {
            let _ = reply.send(CollabOpenResponse::OpenOnline);
        }
    });

    let pid = std::process::id();
    let ops2 = ops.clone();
    let result =
        tokio::task::spawn_blocking(move || ops2.open_file(file_ino, Some(pid), Some("test.docx")))
            .await
            .unwrap();

    assert!(
        result.is_err(),
        "open_file should return an error for OpenOnline"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, cloudmount_vfs::core_ops::VfsError::CollabRedirect),
        "error should be CollabRedirect, got: {err:?}"
    );

    cleanup(&base);
}
