# cloudmount-cache

Multi-tier caching: memory (DashMap, TTL) → SQLite (metadata) → disk (content blobs). Write-back buffer for pending uploads. Delta sync from Microsoft Graph API.

## STRUCTURE

```
src/
├── lib.rs        # Re-exports CacheManager, DeltaSyncTimer
├── manager.rs    # CacheManager — aggregates all cache tiers
├── memory.rs     # In-memory LRU cache (DashMap, inode-keyed)
├── sqlite.rs     # SQLite store — item metadata, delta tokens, sync state
├── disk.rs       # Disk content cache — LRU eviction by total byte size
├── writeback.rs  # Pending upload buffer (filesystem-backed, survives crashes)
└── sync.rs       # DeltaSyncTimer — periodic delta queries from Graph API
tests/
└── cache_tests.rs  # 23 tests covering all tiers + crash recovery
```

## WHERE TO LOOK

| Task | File | Notes |
|------|------|-------|
| Add cached field | `sqlite.rs` → `create_tables` | Add column + update `upsert_item` params |
| Change memory eviction | `memory.rs` → `maybe_evict` | LRU by `last_access`, evicts 10k→8k |
| Change disk eviction | `disk.rs` → `evict_if_needed` | LRU by `last_access`, evicts by byte size |
| New cache tier | `manager.rs` | Add field to `CacheManager`, wire in `new()` |
| Change sync interval | `sync.rs` → `DeltaSyncTimer::start` | `interval_secs` param |
| Track upload state | `writeback.rs` | Files at `pending/{drive_id}/{item_id}` |
| Modify delta application | `sqlite.rs` → `apply_delta` | Single transaction: upserts + deletes + token update |

## TIER ARCHITECTURE

1. **MemoryCache**: `DashMap<u64, CachedEntry>` keyed by inode. 60s TTL default. Evicts LRU when >10k entries (down to 8k). Stores `DriveItem` + optional children inode list.
2. **SqliteStore**: `Mutex<Connection>`. WAL mode + NORMAL sync. Tables: `items` (metadata, JSON blob), `delta_tokens`, `sync_state`, `cache_entries`. All writes use UPSERT.
3. **DiskCache**: Content blobs at `{base_dir}/{drive_id}/{item_id}`. Tracks size via separate SQLite `cache_entries` table. LRU eviction when exceeding `max_size_bytes`.
4. **WriteBackBuffer**: Pending uploads at `{cache_dir}/pending/{drive_id}/{item_id}`. Filesystem-backed — survives process crashes. `list_pending()` on startup for recovery.
5. **DeltaSyncTimer**: `tokio::spawn` loop with `CancellationToken`. Per-drive delta queries. Applies changes transactionally. Handles 410 Gone (expired token → full re-sync).

## CONVENTIONS

- SQLite connection wrapped in `Mutex` (not `RwLock`) — all ops take `&self`.
- `DiskCache` opens its own SQLite connection at same `db_path` as `SqliteStore`.
- Delta sync applies upserts + deletes + token update in single transaction (`unchecked_transaction`).
- All errors map to `cloudmount_core::Error::Cache(String)`.
- Memory cache `insert` always calls `maybe_evict()` first.

## ANTI-PATTERNS

- Do NOT use `async` in SqliteStore — `rusqlite::Connection` is not `Send`.
- Do NOT skip `maybe_evict()` before memory cache inserts.
- Do NOT change SQLite pragmas (WAL + NORMAL) without benchmarking.
- Do NOT access `tracker` connection from DiskCache without `lock()`.
