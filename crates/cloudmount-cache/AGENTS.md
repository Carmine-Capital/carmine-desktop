# cloudmount-cache

Multi-tier caching: memory (DashMap, TTL) → SQLite (metadata) → disk (content blobs). Write-back buffer for pending uploads. Delta sync from Microsoft Graph API.

## CONVENTIONS

- SQLite connection wrapped in `Mutex` (not `RwLock`) — all ops take `&self`.
- Memory cache `insert` always calls `maybe_evict()` first.
- All errors map to `cloudmount_core::Error::Cache(String)`.

## ANTI-PATTERNS

- Do NOT use `async` in SqliteStore — `rusqlite::Connection` is not `Send`.
- Do NOT skip `maybe_evict()` before memory cache inserts.
- Do NOT change SQLite pragmas (WAL + NORMAL) without benchmarking.
- Do NOT access `tracker` connection from DiskCache without `lock()`.
