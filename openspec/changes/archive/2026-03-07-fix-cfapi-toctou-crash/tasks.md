## 1. Refactor fetch_placeholders to per-item loop

- [x] 1.1 In `cfapi.rs::fetch_placeholders`, replace the single `pass_with_placeholder(&mut placeholders)` call with a `for` loop that calls `ticket.pass_with_placeholder(&mut [placeholder])` for each item individually
- [x] 1.2 After each per-item call, inspect the `Result`: if `Err(e)` and `e.code().0 == 0x8007017cu32 as i32` (ERROR_CLOUD_FILE_INVALID_REQUEST), log `warn!(item = %item.name, "cfapi: placeholder already exists (TOCTOU skip)")` and `continue`
- [x] 1.3 For any other `Err(e)`, return `Err(CloudErrorKind::Unsuccessful)` as before (genuine API failure path)
- [x] 1.4 Retain the existing `.filter(|..| !dir_path.join(&item.name).exists())` pre-filter before building the `for` loop (keep as optimisation hint)
- [x] 1.5 Remove the now-unnecessary `if placeholders.is_empty() { return Ok(()); }` early-return guard, or adapt it to operate before the loop if the filtered list is empty

## 2. Verification

- [x] 2.1 Run `cargo clippy -p cloudmount-vfs --all-targets --all-features` (Windows target cross-check via `cargo check --target x86_64-pc-windows-msvc -p cloudmount-vfs`) and confirm zero warnings
- [x] 2.2 Run `cargo fmt --all -- --check` and confirm no formatting issues
- [x] 2.3 Confirm the modified function compiles under `#[cfg(target_os = "windows")]` and no Linux/macOS code is affected
