## 1. Windows Lifecycle State Management

- [x] 1.1 Add a Windows-only process-wide active-mount tracker in `crates/cloudmount-vfs/src/cfapi.rs` (atomic counter or mutex-guarded state).
- [x] 1.2 Add helper functions that handle 0->1 and 1->0 transitions for context-menu registration lifecycle.

## 2. Registry Helper Hardening

- [x] 2.1 Update context-menu registration helper to be idempotent when keys already exist.
- [x] 2.2 Update context-menu cleanup helper to treat missing key paths as successful no-op cleanup.

## 3. Mount/Unmount Flow Integration

- [x] 3.1 Update CfApi mount flow to increment lifecycle state only after successful mount setup and invoke registration on first active mount.
- [x] 3.2 Update CfApi unmount flow to decrement lifecycle state and invoke cleanup only when last active mount is removed.
- [x] 3.3 Ensure teardown paths keep lifecycle state consistent even if unregister operations fail.

## 4. Validation and Regression Coverage

- [x] 4.1 Add/extend Windows-focused tests for multi-mount lifecycle scenarios (first mount registers, intermediate unmount keeps menu, final unmount removes menu).
- [x] 4.2 Run project checks and confirm no warnings/regressions for modified crates.
