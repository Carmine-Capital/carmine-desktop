## Why

Three low-effort, zero-risk performance improvements that each independently reduce latency or I/O overhead. Grouped because they share a common trait: each is a config tweak or drop-in replacement with no architectural changes.

## What Changes

- **FUSE mount options**: Add `max_read`, `max_write`, and kernel writeback cache to eliminate the 4KB write bottleneck and increase read batch size from 128KB to 1MB on Linux
- **SQLite prepared statement caching**: Replace `conn.prepare()` with `conn.prepare_cached()` to avoid re-parsing SQL on every cache lookup
- **Graph API `$select`**: Add field selection to `list_children` and `list_root_children` queries to reduce JSON payload size from the server

## Capabilities

### Modified Capabilities

- `virtual-filesystem`: FUSE mount configuration (Linux/macOS only)
- `cache-layer`: SQLite query performance
- `graph-client`: API query efficiency

## Impact

- `crates/cloudmount-vfs/src/fuse_fs.rs`: Mount options (~5 lines)
- `crates/cloudmount-cache/src/sqlite.rs`: `prepare` -> `prepare_cached` (~5 call sites)
- `crates/cloudmount-graph/src/client.rs`: `$select` parameter on list queries (~2 URLs)

## Risk

Near-zero. Each change is independently testable and reversible. No new dependencies. No API surface changes.
