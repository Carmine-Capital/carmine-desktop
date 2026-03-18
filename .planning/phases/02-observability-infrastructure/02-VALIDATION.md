---
phase: 2
slug: observability-infrastructure
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-18
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust integration tests (`#[tokio::test]`) |
| **Config file** | none — test convention is `crates/<name>/tests/*.rs` |
| **Quick run command** | `toolbox run -c carminedesktop-build cargo test --all-targets -p carminedesktop-cache -p carminedesktop-app` |
| **Full suite command** | `toolbox run -c carminedesktop-build cargo test --all-targets` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `toolbox run -c carminedesktop-build cargo test --all-targets -p carminedesktop-cache -p carminedesktop-app`
- **After every plan wave:** Run `toolbox run -c carminedesktop-build cargo test --all-targets` + `toolbox run -c carminedesktop-build cargo clippy --all-targets --all-features`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | SC-1 | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_get_dashboard_status` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | SC-2 | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_get_recent_errors` | ❌ W0 | ⬜ pending |
| 02-01-03 | 01 | 1 | SC-3 | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_cache_stats` | ❌ W0 | ⬜ pending |
| 02-01-04 | 01 | 1 | SC-4 | manual-only | Manual: subscribe with `listen()` in browser console during delta sync | N/A | ⬜ pending |
| 02-01-05 | 01 | 1 | N/A | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_error_accumulator` | ❌ W0 | ⬜ pending |
| 02-01-06 | 01 | 1 | N/A | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-app test_activity_buffer` | ❌ W0 | ⬜ pending |
| 02-01-07 | 01 | 1 | N/A | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_cache_manager_stats` | ❌ W0 | ⬜ pending |
| 02-01-08 | 01 | 1 | N/A | unit | `toolbox run -c carminedesktop-build cargo test -p carminedesktop-cache test_pin_health` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/carminedesktop-app/tests/observability_tests.rs` — stubs for SC-1, SC-2, ring buffer tests (ErrorAccumulator, ActivityBuffer)
- [ ] `crates/carminedesktop-cache/tests/cache_stats_tests.rs` — stubs for SC-3, CacheManager::stats(), PinStore::health()
- [ ] No framework install needed — existing test infrastructure sufficient

*Note: testing Tauri commands requires either mocking AppState or testing the underlying data structures directly (ring buffers, stat methods). The Tauri command integration (actual IPC) is manual-only.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real-time events pushed via `emit()` | SC-4 | Requires running Tauri app with active WebView to subscribe with `listen()` | 1. Start app with mounted drive. 2. Open browser console. 3. Run `await listen('obs-event', e => console.log(e.payload))`. 4. Trigger delta sync or VFS operation. 5. Verify events appear in console. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
