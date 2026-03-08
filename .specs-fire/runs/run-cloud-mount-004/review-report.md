# Code Review Report — clear-mounts-on-sign-out

**Run:** run-cloud-mount-004

## Files Reviewed

- `crates/cloudmount-app/src/commands.rs` (modified)
- `crates/cloudmount-app/tests/integration_tests.rs` (modified)

## Findings

| Severity | Finding | Action |
|----------|---------|--------|
| N/A | No issues found | — |

## Analysis

**commands.rs**: `user_config.mounts.clear()` is placed correctly between `accounts.clear()` and
`save_to_file`. This is the minimal, correct fix with no side effects. The existing
`rebuild_effective_config` call that follows propagates the empty mounts to `effective_config`.

**integration_tests.rs**: Test updated to simulate `mounts.clear()` in the sign-out flow and
assert `reloaded.mounts.is_empty()`. The simulation now accurately reflects the actual command logic.

## Auto-fixes Applied

None needed.

## Verdict

APPROVED — minimal, correct, no issues.
