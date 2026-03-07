## CHANGED Requirements

### Requirement: SQLite prepared statement caching

All SQLite queries on hot paths must use cached prepared statements to avoid re-parsing SQL on every call.

#### Scenario: Repeated queries reuse prepared statements

- **WHEN** `get_item_by_id`, `get_children`, `get_delta_token`, or `upsert_item` is called multiple times
- **THEN** each call uses `conn.prepare_cached()` instead of `conn.prepare()`
- **AND** rusqlite's internal LRU cache stores the compiled statement for reuse
- **AND** no functional behavior changes — only the preparation path differs
