## Context

Three independent, low-risk performance improvements. No architectural changes — each is a config tweak or drop-in replacement.

## Goals / Non-Goals

**Goals:**
- Reduce per-read FUSE I/O overhead on Linux by requesting 1MB reads from kernel
- Enable kernel write coalescing and parallel directory ops via FUSE capabilities
- Eliminate repeated SQLite statement parsing on hot paths
- Reduce Graph API payload size for directory listings

**Non-Goals:**
- Changing any public API or data structures
- Adding new dependencies
- Modifying behavior on Windows (CfApi is unaffected)

## Decisions

### D1: FUSE capabilities via init(), not mount options

`FUSE_WRITEBACK_CACHE` and `FUSE_PARALLEL_DIROPS` are set in the `Filesystem::init()` callback via `config.add_capabilities()`, NOT via mount options. This is how fuser 0.17 exposes them.

Key findings from fuser 0.17.0 source:
- `big_writes` is already in default INIT_FLAGS on Linux (line 108)
- `max_write` defaults to 16MB via `MAX_WRITE_SIZE` — already sufficient
- `max_read` is a kernel-side limit, set via `CUSTOM("max_read=1048576")` mount option
- `add_capabilities()` returns `Err` with unsupported bits if the kernel doesn't support a capability — we log a warning and continue (graceful degradation)

`fuser` re-exports `InitFlags` publicly from `fuser::InitFlags`.

### D2: prepare_cached is a drop-in replacement

`rusqlite::Connection::prepare_cached()` has the same signature as `prepare()` but returns a `CachedStatement` that auto-returns to the LRU cache on drop. No code changes beyond the method name. Applied to all `conn.prepare()` call sites in `sqlite.rs`.

### D3: $select only on list queries, not delta

`$select` is added only to `list_children` and `list_root_children`. Delta queries are excluded because:
- Delta responses include `deleted` facets not in our standard field set
- The `@odata.nextLink` from delta already includes server-chosen parameters
- Delta queries return incremental changes (typically small), so payload reduction matters less

The selected fields match what `DriveItem` actually deserializes:
`id,name,size,lastModifiedDateTime,createdDateTime,eTag,parentReference,folder,file,@microsoft.graph.downloadUrl`
