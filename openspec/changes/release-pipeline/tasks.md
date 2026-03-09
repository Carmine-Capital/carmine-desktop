## 1. Tauri Configuration

- [x] 1.1 Update `tauri.conf.json`: switch Windows bundler from `wix` to `nsis`
- [x] 1.2 Update `tauri.conf.json`: set `plugins.updater.endpoints` to `["https://github.com/{owner}/{repo}/releases/latest/download/latest.json"]` (replace with actual owner/repo)
- [x] 1.3 Update `tauri.conf.json`: set `plugins.updater.pubkey` to the generated ed25519 public key (placeholder until key is generated in task 2.1)

## 2. Updater Signing Key

- [x] 2.1 Generate Tauri updater key pair with `cargo tauri signer generate` and record the public key
- [x] 2.2 Add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub repository secrets

## 3. Release Workflow

- [x] 3.1 Create `.github/workflows/release.yml` with `on: push: tags: ['v*']` trigger
- [x] 3.2 Add version-tag consistency check job: extract version from `tauri.conf.json`, compare to git tag, fail on mismatch
- [x] 3.3 Add `create-release` job: create a draft GitHub Release using `softprops/action-gh-release@v2`
- [x] 3.4 Add `build` job with matrix: `ubuntu-latest` (x86_64-unknown-linux-gnu), `macos-latest` (aarch64-apple-darwin), `macos-13` (x86_64-apple-darwin), `windows-latest` (x86_64-pc-windows-msvc)
- [x] 3.5 Add platform-specific dependency installation steps (same as `ci.yml`: libfuse3-dev + webkit deps on Linux, macFUSE on macOS)
- [x] 3.6 Wire `tauri-apps/tauri-action@v0` in each matrix build with `--features desktop`, `TAURI_SIGNING_PRIVATE_KEY`, and `releaseId` from the create-release job
- [x] 3.7 Add `publish-release` job: un-draft the release after all builds succeed

## 4. Validation

- [ ] 4.1 Push a test tag (e.g., `v0.1.0-rc.1`) and verify the workflow runs, builds all platforms, and publishes the release with all expected artifacts and `latest.json`
- [ ] 4.2 Verify `latest.json` contains correct platform entries with download URLs and signatures
