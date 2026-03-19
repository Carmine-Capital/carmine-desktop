# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — Stabilization & Observability

**Shipped:** 2026-03-19
**Phases:** 4 | **Plans:** 10

### What Was Built
- WinFsp offline pin crash fix (5s VFS timeout + memory eviction protection + SQLite metadata population)
- Full observability infrastructure (ObsEvent bus, ring buffers, 4 Tauri dashboard commands)
- 6-section dashboard UI with real-time updates and 30s periodic refresh
- CSS design system refresh (soft dark palette, 4-tier typography, normalized spacing)
- Inline style migration to CSS classes + action feedback improvements

### What Worked
- TDD approach for VFS timeout and cache eviction — tests caught serde rename_all bug on tagged enums early
- Building observability infra (Phase 2) before UI (Phase 3) — data contracts verified from browser console before any frontend work
- Zero new dependencies policy — broadcast channels, ring buffers, and all IPC used existing workspace capabilities
- Phase 2 browser console verification checkpoint (Plan 02-04) caught nothing because the implementation was solid — but would have caught issues cheaply
- Coarse granularity worked well for a 2-day sprint — phases mapped cleanly to logical boundaries

### What Was Inefficient
- Pin health inode chain bug required 3 fix commits during Phase 3 — root cause was temp inodes from pin_folder not matching VFS-browsed inodes. Could have been caught earlier with an integration test for pin-then-browse scenarios
- ROADMAP.md had inconsistent plan checkboxes (some plans marked `[ ]` in roadmap despite having SUMMARY.md files) — tooling didn't auto-update roadmap checkboxes
- UI-02 visual verification checkpoint was never completed by user — requirement marked Pending at milestone close

### Patterns Established
- `graph_with_timeout` helper for all VFS-path Graph API calls — centralizes timeout + offline-flag logic
- ObsEvent tagged union with per-field serde rename (not container rename_all) for correct camelCase
- Lock ordering documentation on AppState for deadlock prevention
- Dashboard command pattern: snapshot-then-release for Mutex-guarded state
- Data refresh pattern: real-time obs-event for immediate changes + periodic refresh for staleness
- Inline style migration workflow: add CSS classes in one plan, apply to JS/HTML in the next

### Key Lessons
1. Serde `rename_all` on tagged enums only renames variant names, not inner fields — always test JSON serialization against expected output
2. SQLite inode management requires careful coordination when multiple code paths (VFS browsing, offline pin) can create entries for the same items — read back actual DB state after upsert
3. Building data layer before UI pays off — all 4 Tauri commands worked on first browser console test because types and contracts were validated early

### Cost Observations
- Sessions: ~4 (2 days of work)
- Notable: All 10 plans executed with zero deviations from plan (except 5 auto-fixed bugs caught during execution)

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Phases | Plans | Key Change |
|-----------|--------|-------|------------|
| v1.0 | 4 | 10 | Initial milestone — established TDD, observability-first, zero-dep patterns |

### Cumulative Quality

| Milestone | Tests Added | Zero-Dep Additions | Auto-Fixed Bugs |
|-----------|-------------|-------------------|-----------------|
| v1.0 | ~20 | 0 | 5 |

### Top Lessons (Verified Across Milestones)

1. Build data layer before UI — contracts verified early prevent frontend rework
2. Zero new dependencies reduces integration risk and keeps build times stable
