# State: CarmineDesktop — Stabilization & Observability

## Project Reference

**Core Value:** When something goes wrong, you know about it and can diagnose it — the app is transparent, not a black box.
**Current Focus:** Phase 1 — WinFsp Offline Pin Fix (deployment blocker)

## Current Position

**Phase:** 1 of 4 — WinFsp Offline Pin Fix
**Plan:** Not yet planned
**Status:** Not started
**Progress:** ░░░░░░░░░░ 0%

## Performance Metrics

| Metric | Value |
|--------|-------|
| Phases completed | 0/4 |
| Plans completed | 0/? |
| Requirements delivered | 0/15 |

## Accumulated Context

### Key Decisions

| Decision | Rationale | Phase |
|----------|-----------|-------|
| Fix offline pins before features | Crash is deployment blocker for Windows rollout | Phase 1 |
| Build observability infra before UI | Data layer must be queryable/testable before building views | Phase 2 |
| Dashboard as panel in settings page | Single UI surface, follows existing vanilla JS pattern | Phase 3 |
| Zero new dependencies | All capabilities exist in workspace (Tauri IPC, tokio broadcast, tracing Layer) | All |

### Todos

(none yet)

### Blockers

(none yet)

### Gotchas

- WinFsp offline crash root cause is unconfirmed — Phase 1 must start with investigation before committing to fix strategy
- 56 occurrences of `.lock().unwrap()` in codebase — lock ordering must be documented during Phase 2
- `main.rs` is 2167 lines — may need refactoring to add observability hooks
- CSP constraint: `script-src 'self'` — no inline event handlers, use `addEventListener` only

## Session Continuity

### Last Session

(no sessions yet)

### Resume Prompt

Start planning Phase 1: WinFsp Offline Pin Fix. Review the research SUMMARY.md Phase 1 section and the codebase around `CoreOps::resolve_path`, `pin_folder`, and WinFsp offline behavior.

---
*State initialized: 2026-03-18*
*Last updated: 2026-03-18*
