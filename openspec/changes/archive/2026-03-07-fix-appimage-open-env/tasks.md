## 1. Extract helper function

- [x] 1.1 Add `pub(crate) fn open_with_clean_env(path: &str) -> Result<(), String>` to `main.rs`, platform-gated: on Linux uses `Command::new("xdg-open").env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD").status()`; on other platforms uses `open::that(path).map_err(|e| e.to_string())`

## 2. Fix call sites

- [x] 2.1 Simplify the desktop `OpenerFn` lambda (`main.rs:409-421`) to call `open_with_clean_env(url)`, removing the duplicated inline `Command::new("xdg-open")` block
- [x] 2.2 Update headless `OpenerFn` lambda (`main.rs:942`) to call `open_with_clean_env(url)` instead of `open::that(url)`
- [x] 2.3 Replace `open::that(&expanded)` at `tray.rs:72` with `let _ = crate::open_with_clean_env(&expanded);`

## 3. Verify

- [x] 3.1 Run `cargo clippy --all-targets --all-features` — zero warnings
- [x] 3.2 Run `cargo build --all-targets` — clean build
