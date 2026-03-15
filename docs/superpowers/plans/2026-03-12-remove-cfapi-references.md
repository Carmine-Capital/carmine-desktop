# Remove CfApi References Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all remaining CfApi references from non-archived source code, tests, agent files, and live specs now that the Windows backend has been replaced by WinFsp.

**Architecture:** CfApi was replaced by WinFsp (in `winfsp_fs.rs`). All references to the old `cfapi.rs` backend, `CfMountHandle`, and CfApi-specific config/migration code must be updated or deleted. Archived specs in `openspec/changes/archive/` are intentionally left untouched as historical record.

**Tech Stack:** Rust, TOML config, Markdown docs/specs

---

## Chunk 1: Rust source code

### Task 1: Remove CfApi migration config field

**Files:**
- Modify: `crates/cloudmount-core/src/config.rs:59-74,147-171`
- Modify: `crates/cloudmount-core/tests/config_tests.rs:155-165`

The `cfapi_migrated` field was a one-time migration flag. Its purpose (cleaning up CfApi sync roots after WinFsp migration) is now complete on all existing installations. New installs never had CfApi roots.

- [ ] **Step 1: Remove the field from `UserGeneralSettings`**

In `crates/cloudmount-core/src/config.rs`, remove lines 168–170:

```rust
    /// Whether the CfApi sync root cleanup has been performed after migration to WinFsp.
    #[serde(default)]
    pub cfapi_migrated: Option<bool>,
```

- [ ] **Step 2: Remove the reset_setting arm**

In the same file, remove from `reset_setting` (around line 70):

```rust
                "cfapi_migrated" => g.cfapi_migrated = None,
```

- [ ] **Step 3: Update the config test struct literal**

In `crates/cloudmount-core/tests/config_tests.rs` at line 164, remove the field:

```rust
            cfapi_migrated: None,
```

- [ ] **Step 4: Run tests to verify**

```bash
make test
```

Expected: all config tests pass (the struct literal still compiles because the field is gone).

- [ ] **Step 5: Commit**

```bash
git add crates/cloudmount-core/src/config.rs crates/cloudmount-core/tests/config_tests.rs
git commit -m "feat(config): remove cfapi_migrated migration field"
```

---

### Task 2: Remove migration code from main.rs

**Files:**
- Modify: `crates/cloudmount-app/src/main.rs`

Three locations in `main.rs` reference CfApi:
1. Lines 590–626: one-time migration block inside `setup_after_launch`
2. Lines 943: comment "stale directory left over from CfApi migration"
3. Lines 1368–1403: `cleanup_cfapi_sync_roots()` function

- [ ] **Step 1: Remove the migration block (lines 590–626)**

Delete the entire `#[cfg(target_os = "windows")]` block that reads `cfapi_migrated`, calls `cleanup_cfapi_sync_roots()`, and writes the flag back:

```rust
    // DELETE THIS ENTIRE BLOCK:
    #[cfg(target_os = "windows")]
    {
        let needs_migration = { ... };
        if needs_migration {
            tracing::info!("performing one-time CfApi sync root cleanup after WinFsp migration");
            match cleanup_cfapi_sync_roots() { ... }
            { ... general.cfapi_migrated = Some(true); ... }
        }
    }
```

- [ ] **Step 2: Update the stale-directory comment (line 943)**

Change:
```rust
        // Remove stale directory left over from CfApi migration or a previous run.
```
To:
```rust
        // Remove stale directory left over from a previous run.
```

- [ ] **Step 3: Remove `cleanup_cfapi_sync_roots()` function (lines 1368–1403)**

Delete the entire function including its doc comment:

```rust
/// Attempt to unregister CfApi sync roots left over from the previous backend.
/// Returns the number of roots successfully unregistered.
#[cfg(target_os = "windows")]
fn cleanup_cfapi_sync_roots() -> Result<usize, String> {
    ...
}
```

- [ ] **Step 4: Run clippy + build**

```bash
make clippy
make build
```

Expected: zero warnings, builds successfully.

- [ ] **Step 5: Commit**

```bash
git add crates/cloudmount-app/src/main.rs
git commit -m "feat(app): remove one-time CfApi→WinFsp migration code"
```

---

### Task 3: Fix stale CfApi comment in commands.rs

**Files:**
- Modify: `crates/cloudmount-app/src/commands.rs:645`

- [ ] **Step 1: Update the comment**

Change:
```rust
        true // Windows uses CfApi, always available after preflight
```
To:
```rust
        true // Windows uses WinFsp, always available after preflight
```

- [ ] **Step 2: Fix stale comment in sync.rs**

In `crates/cloudmount-cache/src/sync.rs:64`, change:
```rust
/// (e.g., CfApi placeholder updates on Windows).
```
To:
```rust
/// (e.g., WinFsp placeholder updates on Windows).
```

- [ ] **Step 3: Fix DEVELOPING.md Windows prerequisite note**

In `DEVELOPING.md` around line 23, the line reads:
```
Windows 10 1709+ (Cloud Files API is built-in). No additional dependencies for headless mode.
```
CfApi was built-in to Windows; WinFsp requires a separate installer. Update to reflect this:
```
Windows 10 1709+ with WinFsp installed (https://winfsp.dev/rel/). No additional dependencies for headless mode.
```

- [ ] **Step 4: Commit**

```bash
git add crates/cloudmount-app/src/commands.rs crates/cloudmount-cache/src/sync.rs DEVELOPING.md
git commit -m "docs: update stale CfApi comments and Windows prerequisite to WinFsp"
```

---

### Task 4: Fix stale Windows integration test

**Files:**
- Modify: `crates/cloudmount-app/tests/integration_tests.rs` (around line 1138)

The test `test_smoke_windows_cfapi_mount_list_read_write_unmount` references `CfMountHandle` which no longer exists — `lib.rs` now exports `WinFspMountHandle`. The test also passes arguments that match the old CfApi `mount()` signature, not `WinFspMountHandle::mount()`.

Current `WinFspMountHandle::mount()` signature (from `winfsp_fs.rs:938`):
```rust
pub fn mount(
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    drive_id: String,
    mountpoint: &str,         // was: &Path in CfApi
    rt: Handle,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    sync_handle: Option<crate::sync_processor::SyncHandle>,
) -> cloudmount_core::Result<Self>
```

- [ ] **Step 1: Rename the test and update imports**

Change:
```rust
#[ignore = "requires Windows with Cloud Files API"]
#[cfg(target_os = "windows")]
async fn test_smoke_windows_cfapi_mount_list_read_write_unmount() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::CfMountHandle;
```
To:
```rust
#[ignore = "requires Windows with WinFsp"]
#[cfg(target_os = "windows")]
async fn test_smoke_windows_winfsp_mount_list_read_write_unmount() -> cloudmount_core::Result<()> {
    use cloudmount_vfs::WinFspMountHandle;
```

- [ ] **Step 2: Update the mount call**

The old CfApi call had 9 args including two CfApi-specific positional strings (`account_name` and `display_name`). The new `WinFspMountHandle::mount()` takes 8 args — remove those two extra strings and adjust the path argument.

Change:
```rust
    let mount = CfMountHandle::mount(
        graph,
        cache,
        inodes,
        drive_id.to_string(),
        &sync_root,
        rt,
        drive_id.to_string(),   // CfApi account_name — remove
        "Smoke Mount".to_string(), // CfApi display_name — remove
        None,
    )?;
```
To:
```rust
    let mount = WinFspMountHandle::mount(
        graph,
        cache,
        inodes,
        drive_id.to_string(),
        sync_root.to_str().unwrap(), // WinFsp takes &str, not &Path
        rt,
        None, // event_tx
        None, // sync_handle
    )?;
```

- [ ] **Step 3: Run clippy on windows or verify it compiles**

```bash
make clippy
```

Expected: compiles (the test body is `#[cfg(target_os = "windows")]` so it won't run on Linux, but should compile without type errors on Windows CI).

- [ ] **Step 4: Commit**

```bash
git add crates/cloudmount-app/tests/integration_tests.rs
git commit -m "test: rename cfapi smoke test to winfsp, update type references"
```

---

## Chunk 2: Agent and documentation files

### Task 5: Update root AGENTS.md / CLAUDE.md

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md` (identical content — both must be kept in sync)

Both files contain three CfApi references:
- Line 14: `cloudmount-vfs` description
- Line 21–22: WHERE TO LOOK table
- Lines 30, 33: CONVENTIONS section

- [ ] **Step 1: Update `AGENTS.md`**

Apply these replacements:

| Old | New |
|-----|-----|
| `VFS: FUSE (Linux/macOS), Cloud Files API (Windows)` | `VFS: FUSE (Linux/macOS), WinFsp (Windows)` |
| `Both FUSE and CfApi delegate here` | `Both FUSE and WinFsp delegate here` |
| `FUSE / CfApi backends \| fuse_fs.rs / cfapi.rs` | `FUSE / WinFsp backends \| fuse_fs.rs / winfsp_fs.rs` |
| `FUSE/CfApi trait methods are sync` | `FUSE/WinFsp trait methods are sync` |
| `#[cfg(target_os = "windows")]` for CfApi` | `#[cfg(target_os = "windows")]` for WinFsp` |

- [ ] **Step 2: Apply the same changes to `CLAUDE.md`**

(Same edits — the files are identical.)

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md CLAUDE.md
git commit -m "docs: update root AGENTS.md/CLAUDE.md to reference WinFsp instead of CfApi"
```

---

### Task 6: Update cloudmount-vfs AGENTS.md / CLAUDE.md

**Files:**
- Modify: `crates/cloudmount-vfs/AGENTS.md`
- Modify: `crates/cloudmount-vfs/CLAUDE.md`

Line 3 of both files reads:
> Virtual filesystem exposing OneDrive/SharePoint as local mount. FUSE on Linux/macOS, Cloud Files API (CfApi) on Windows. All platform-gated via `#[cfg]`.

- [ ] **Step 1: Update both files**

Change `Cloud Files API (CfApi)` to `WinFsp` in both `crates/cloudmount-vfs/AGENTS.md` and `crates/cloudmount-vfs/CLAUDE.md`.

Also update line 14 in AGENTS.md:
```
- All `Filesystem`/`SyncFilter` trait methods are sync. Bridge to async via `rt.block_on()`.
```
Remove `SyncFilter` (CfApi term) — replace with:
```
- All `Filesystem`/`FileSystemContext` trait methods are sync. Bridge to async via `rt.block_on()`.
```

- [ ] **Step 2: Commit**

```bash
git add crates/cloudmount-vfs/AGENTS.md crates/cloudmount-vfs/CLAUDE.md
git commit -m "docs(vfs): update VFS crate docs to reference WinFsp instead of CfApi"
```

---

### Task 7: Update vfs-parity-reviewer agent

**Files:**
- Modify: `.claude/agents/vfs-parity-reviewer.md`
- Modify: `.opencode/agents/vfs-parity-reviewer.md`

The agent's description, system prompt, and review scope all refer to CfApi and `cfapi.rs`. Apply these replacements throughout both files:

| Old | New |
|-----|-----|
| `CfApi or FUSE code` | `WinFsp or FUSE code` |
| `FUSE (Linux/macOS) in \`fuse_fs.rs\` and CfApi (Windows) in \`cfapi.rs\`` | `FUSE (Linux/macOS) in \`fuse_fs.rs\` and WinFsp (Windows) in \`winfsp_fs.rs\`` |
| `Functional parity between FUSE and CfApi` | `Functional parity between FUSE and WinFsp` |
| `every CfApi write path` | `every WinFsp write path` |
| `CfApi \`delete()\`` | `WinFsp \`delete()\`` |
| `CfApi propagate errors...via \`CResult<>()\` / \`CloudErrorKind\`` | `WinFsp propagate errors to the OS via NTSTATUS codes (\`STATUS_*\`)` |
| `FUSE, CfApi, or both` | `FUSE, WinFsp, or both` |
| `crates/cloudmount-vfs/src/cfapi.rs — Windows CfApi backend` | `crates/cloudmount-vfs/src/winfsp_fs.rs — Windows WinFsp backend` |

- [ ] **Step 1: Update `.claude/agents/vfs-parity-reviewer.md`**

- [ ] **Step 2: Update `.opencode/agents/vfs-parity-reviewer.md`**

- [ ] **Step 3: Commit**

```bash
git add .claude/agents/vfs-parity-reviewer.md .opencode/agents/vfs-parity-reviewer.md
git commit -m "docs(agents): update vfs-parity-reviewer to reference WinFsp"
```

---

### Task 8: Update cross-platform-reviewer agent

**Files:**
- Modify: `.claude/agents/cross-platform-reviewer.md`
- Modify: `.opencode/agents/cross-platform-reviewer.md`

Three references to update:

| Old | New |
|-----|-----|
| `targeting Linux (FUSE), macOS (FUSE), and Windows (CfApi)` | `targeting Linux (FUSE), macOS (FUSE), and Windows (WinFsp)` |
| `Windows-specific: CfApi callback patterns, sync root lifecycle` | `Windows-specific: WinFsp \`FileSystemContext\` trait patterns, mount/volume lifecycle` |
| `platform-gated code (\`cfapi.rs\`, Windows-only modules...)` | `platform-gated code (\`winfsp_fs.rs\`, Windows-only modules...)` |

- [ ] **Step 1: Update `.claude/agents/cross-platform-reviewer.md`**

- [ ] **Step 2: Update `.opencode/agents/cross-platform-reviewer.md`**

- [ ] **Step 3: Commit**

```bash
git add .claude/agents/cross-platform-reviewer.md .opencode/agents/cross-platform-reviewer.md
git commit -m "docs(agents): update cross-platform-reviewer to reference WinFsp"
```

---

## Chunk 3: Live OpenSpec specs

> **Note on spec constraints:** `CLAUDE.md` marks `openspec/specs/` as read-only for routine work. The user has explicitly requested this cleanup — that constitutes explicit permission to modify these files directly.

### Task 9: Delete dead CfApi-specific specs

These four specs describe features (`SyncFilter`, local-change watcher, periodic timer, post-upload conversion) that belonged exclusively to the CfApi backend. WinFsp implements equivalent behavior differently and has its own spec (`winfsp-filesystem/spec.md`). These dead specs are confusing because they describe non-existent code.

**Files to delete:**
- `openspec/specs/cfapi-local-change-watcher/` (entire directory)
- `openspec/specs/cfapi-periodic-timer/` (entire directory)
- `openspec/specs/cfapi-post-upload-conversion/` (entire directory)
- `openspec/specs/cfapi-local-change-sync/` (entire directory)
- `openspec/specs/cfapi-placeholder-sync/` (entire directory)

- [ ] **Step 1: Verify no live code references these spec files**

```bash
grep -r "cfapi-local-change-watcher\|cfapi-periodic-timer\|cfapi-post-upload-conversion\|cfapi-local-change-sync" \
  --include="*.rs" --include="*.md" \
  --exclude-dir="openspec/changes/archive" \
  .
```

Expected: zero matches in non-archived locations (archives are OK).

- [ ] **Step 2: Delete the spec directories**

```bash
rm -rf openspec/specs/cfapi-local-change-watcher
rm -rf openspec/specs/cfapi-periodic-timer
rm -rf openspec/specs/cfapi-post-upload-conversion
rm -rf openspec/specs/cfapi-local-change-sync
rm -rf openspec/specs/cfapi-placeholder-sync
```

- [ ] **Step 3: Commit**

```bash
git add -A openspec/specs/
git commit -m "docs(specs): delete defunct CfApi-specific specs (replaced by WinFsp)"
```

---

### Task 10: Update live specs that reference CfApi inline

**Files:**
- Modify: `openspec/specs/virtual-filesystem/spec.md`
- Modify: `openspec/specs/app-lifecycle/spec.md`
- Modify: `openspec/specs/platform-preflight/spec.md`
- Modify: `openspec/specs/windows-context-menu-lifecycle/spec.md`
- Modify: `openspec/specs/open-in-sharepoint/spec.md`
- Modify: `openspec/specs/cache-layer/spec.md`
- Modify: `openspec/specs/winfsp-filesystem/spec.md` (partial — see note)

These are active specs that describe current behavior but use CfApi terminology. The changes are purely textual replacements — the described behavior is correct, just the backend name is wrong.

**virtual-filesystem/spec.md** — occurrences to change:
| Old | New |
|-----|-----|
| `FUSE/CfApi` | `FUSE/WinFsp` |
| `on Windows (CfApi)` | `on Windows (WinFsp)` |
| `### Requirement: CfApi fetch_data immediate failure signaling` | `### Requirement: WinFsp read failure signaling` |
| `### Requirement: CfApi closed callback surfaces upload failures` | `### Requirement: WinFsp cleanup callback surfaces upload failures` |
| `### Requirement: CfApi writeback failure notification` | `### Requirement: WinFsp writeback failure notification` |
| `CfApi \`closed\` callback` | `WinFsp \`cleanup\` callback` |
| `CfApi mount is registered` | `WinFsp mount is registered` |
| `CfApi backend's existing \`WritebackFailed\` emission` | `WinFsp backend's existing \`WritebackFailed\` emission` |
| `not registered with FUSE/CfApi` | `not registered with FUSE/WinFsp` |

**app-lifecycle/spec.md** — occurrences to change:
| Old | New |
|-----|-----|
| `FUSE or CfApi session` | `FUSE or WinFsp session` |
| `CfApi \`CfMountHandle\`` | `WinFsp \`WinFspMountHandle\`` |
| `FUSE/CfApi sessions` | `FUSE/WinFsp sessions` |
| `starts a CfApi mount on Windows` | `starts a WinFsp mount on Windows` |

**platform-preflight/spec.md** — replace all `CfApi` with `WinFsp`.

**windows-context-menu-lifecycle/spec.md** — replace all `CfApi mounts` with `WinFsp mounts` and any other CfApi terminology with WinFsp equivalents.

**open-in-sharepoint/spec.md** — replace `CfApi sync root is registered/unregistered` with `WinFsp mount is registered/unmounted`.

**cache-layer/spec.md** — replace `CfApi placeholder updates on Windows` with `WinFsp placeholder updates on Windows`.

**winfsp-filesystem/spec.md** — this spec has a formal requirement titled "CfApi sync root cleanup on upgrade" (around lines 299–311) that describes the `cfapi_migrated` migration which Task 1 and Task 2 remove from the implementation. Since the implementation is now removed, delete or replace that requirement section. Replace it with a one-line note:
```markdown
> **Historical note:** CfApi sync root cleanup was performed as a one-time migration step in v0.x. The `cfapi_migrated` config flag and `cleanup_cfapi_sync_roots()` function were removed once migration was complete.
```

- [ ] **Step 1: Edit each spec file listed above**

For each file, apply the substitutions from the tables above. After each edit, verify no CfApi references remain:

```bash
for f in \
  openspec/specs/virtual-filesystem/spec.md \
  openspec/specs/app-lifecycle/spec.md \
  openspec/specs/platform-preflight/spec.md \
  openspec/specs/windows-context-menu-lifecycle/spec.md \
  openspec/specs/open-in-sharepoint/spec.md \
  openspec/specs/cache-layer/spec.md \
  openspec/specs/winfsp-filesystem/spec.md; do
  echo "=== $f ==="; grep -in "cfapi\|cf_api" "$f" || echo "(clean)"
done
```

Expected: `(clean)` for every file.

- [ ] **Step 2: Final scan — confirm no CfApi references remain in non-archived files**

```bash
grep -ri "cfapi\|cf_api\|cloud.files.api" \
  --include="*.rs" --include="*.md" --include="*.toml" \
  --exclude-dir="openspec/changes/archive" \
  . \
  | grep -v "Binary file"
```

Expected: zero matches. If any remain, fix before committing.

- [ ] **Step 3: Run full build and clippy**

```bash
make clippy && make test
```

Expected: zero warnings, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add openspec/specs/
git commit -m "docs(specs): replace CfApi terminology with WinFsp across all live specs"
```
