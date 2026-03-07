## 1. Harden fetch_data callback

- [x] 1.1 In `cfapi.rs::fetch_data`, replace the `resolve_path(...).ok_or(CloudErrorKind::NotInSync)?` line with: if `resolve_path` returns `None`, log `tracing::warn!(path = %rel_path, "cfapi: fetch_data could not resolve path, skipping")` and `return Ok(())`
- [x] 1.2 Replace the `read_range_direct(...).map_err(|_| CloudErrorKind::Unsuccessful)?` line with: if `read_range_direct` returns `Err(e)`, log `tracing::warn!(path = %rel_path, "cfapi: fetch_data download failed: {e}")` and `return Ok(())`
- [x] 1.3 Replace `ticket.write_at(...).map_err(|_| CloudErrorKind::Unsuccessful)?` inside the write loop with: if `write_at` returns `Err(e)`, log `tracing::warn!(path = %rel_path, "cfapi: fetch_data write_at failed: {e:?}")`, break out of the loop, and fall through to `return Ok(())`
- [x] 1.4 Confirm `fetch_data` now returns `Ok(())` on all code paths (no remaining `?` propagation and no `return Err(...)`)

## 2. Harden dehydrate, delete, rename callbacks

- [x] 2.1 In `cfapi.rs::dehydrate`, replace `ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?` with `if let Err(e) = ticket.pass() { tracing::warn!("cfapi: dehydrate ticket.pass() failed: {e:?}"); }`; remove the `Ok(())` trailing line if it becomes unreachable, or keep it — either compiles
- [x] 2.2 In `cfapi.rs::delete`, replace `ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?` with the same log-and-continue pattern as 2.1 (include the relative path in the warn field if available)
- [x] 2.3 In `cfapi.rs::rename`, replace `ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?` with the same log-and-continue pattern as 2.1 (include source and target paths in the warn field)
- [x] 2.4 Confirm all three callbacks now have return type `CResult<()>` but no `Err`-returning code path remains (all `?` and `return Err` removed from the ticket.pass() site)

## 3. Fix integration test placeholder state

- [x] 3.1 In `cfapi_integration.rs::create_root_placeholders`, remove `.mark_in_sync()` from the `hello.txt` `PlaceholderFile` chain, leaving the file placeholder dehydrated (metadata + blob only)
- [x] 3.2 Keep `.mark_in_sync()` on the `docs` directory placeholder (directories do not need to be hydrated, so in-sync is correct)
- [ ] 3.3 Verify that `cfapi_browse_populates_placeholders` still passes after the change (directory listing does not depend on hydration state)

## 4. Verification

- [x] 4.1 Run `cargo check --target x86_64-pc-windows-msvc -p cloudmount-vfs` and confirm zero errors
- [x] 4.2 Run `cargo clippy --target x86_64-pc-windows-msvc -p cloudmount-vfs --all-targets --all-features` and confirm zero warnings
- [x] 4.3 Run `cargo fmt --all -- --check` and confirm no formatting issues
- [ ] 4.4 Confirm all six integration tests in `cfapi_integration.rs` pass on a Windows CI run: `cargo test -p cloudmount-vfs --test cfapi_integration -- --ignored`
