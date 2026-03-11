## 1. KDE Service Menu Assets

- [x] 1.1 Add a Dolphin Service Menu `.desktop` file defining an `Open in SharePoint` action for file selections
- [x] 1.2 Wire the Service Menu action to invoke a CloudMount helper script with selected file paths
- [x] 1.3 Ensure the action metadata works for single and multi-file selection in Dolphin

## 2. Linux Helper Script for Dolphin

- [x] 2.1 Add a helper script that iterates selected paths provided by Dolphin and skips empty entries safely
- [x] 2.2 Percent-encode each absolute path and build `cloudmount://open-online?path=<encoded>` URLs
- [x] 2.3 Dispatch each deep link through `xdg-open` and preserve per-file failure isolation

## 3. Documentation and Packaging Notes

- [x] 3.1 Document KDE setup instructions (Service Menu location, executable permissions, restart/reload steps)
- [x] 3.2 Document prerequisites and troubleshooting (CloudMount running, deep-link registration, `xdg-open` availability)
- [x] 3.3 Clarify coexistence with the existing Nautilus integration and expected KDE behavior
- [x] 3.4 Document multi-selection behavior and Linux instance-handling caveat (best-effort dispatch, possible multiple launches)

## 4. Behavior Verification

- [ ] 4.1 Validate single-file flow in Dolphin for an Office file and confirm SharePoint/Office Online opens
- [ ] 4.2 Validate multi-selection flow in Dolphin and confirm one deep-link dispatch per file
- [ ] 4.3 Validate a path outside CloudMount mounts and confirm user-visible error notification

## 5. Regression and Quality Checks

- [x] 5.1 Verify existing Nautilus script behavior is unchanged after KDE integration assets are added
- [x] 5.2 Run project checks (`make check`) and resolve any warnings or failures
