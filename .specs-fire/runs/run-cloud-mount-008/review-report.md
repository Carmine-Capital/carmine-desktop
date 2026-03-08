# Cross-Platform Review Report — run-cloud-mount-008

**Reviewed files:**
- `crates/cloudmount-core/src/config.rs`
- `crates/cloudmount-app/src/main.rs` (lines 585–807, 1135–1245)

---

## 1. `derive_mount_point` — CORRECT

The new implementation correctly replaces `format!("{home}/{root_dir}/...")` with
`std::path::Path::new(&home).join(root_dir)` followed by `.join(...)` calls.
`Path::join` uses `std::path::MAIN_SEPARATOR` internally, so no hardcoded `/` survives
onto Windows. The `to_string_lossy().into_owned()` round-trip is the right idiom when
the result must be a `String` for storage in `MountConfig::mount_point`.

**`to_string_lossy` lossiness risk:** On all three target platforms (Linux, macOS,
Windows) the constructed path components — home dir, `root_dir`, mount-type literals
— are all valid UTF-8. `dirs::home_dir()` can theoretically return a non-UTF-8 path on
Linux (arbitrary bytes in `$HOME`), but this is an edge case that existed implicitly
before (the old `p.to_string_lossy().to_string()` call for `home`) and is acceptable
for a desktop application whose config round-trips through TOML (which requires UTF-8).
No regression introduced.

**Verdict: no issues.**

---

## 2. `expand_mount_point` — ONE ISSUE FOUND

### 2a. `~/...` branch — correct

```rust
std::path::Path::new(&home).join(rest).to_string_lossy().into_owned()
```

`rest` is the portion after `~/` (e.g. `"Cloud/OneDrive"`). On Windows,
`Path::join` will accept forward-slash component separators within `rest` only if the
string does not start with a drive letter or `\\`; since `rest` comes from a user-typed
TOML value (relative path fragment), this is safe. No issue.

### 2b. `~ alone` branch — correct

Returns `home` directly. No issue.

### 2c. `{home}` substitution in the `else` branch — ISSUE

```rust
} else {
    template.replace("{home}", &home)
}
```

The `home` value obtained from `dirs::home_dir()` is already a native path (e.g.
`C:\Users\Alice` on Windows). The caller substitutes that raw string into `template`
with a simple `str::replace`. If `template` is something like
`"{home}/Cloud/OneDrive"` (forward-slash), the result becomes
`C:\Users\Alice/Cloud/OneDrive` — a mixed-separator path — exactly the original bug
that motivated this change.

However, this else-branch is only reachable when `template` contains `{home}` but does
**not** start with `~/` and is not `~`. That means the template itself has an embedded
`{home}` placeholder, e.g. `"{home}/Cloud"`. After replacement the path has mixed
separators on Windows.

**Severity:** Low-medium. This path is not exercised by `derive_mount_point` (which
never emits a `{home}` template) and is not the common case for auto-derived mounts,
but it is the documented contract of `expand_mount_point` and a user could write
`mount_point = "{home}/Cloud/OneDrive"` in `config.toml`.

**Fix:** Parse `home` and the post-substitution fragment through `Path::join` rather
than relying on `str::replace`:

```rust
} else {
    // Replace {home} token then re-normalise separators.
    let substituted = template.replace("{home}", "");
    // Strip any leading separator left after removing {home}.
    let rest = substituted.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home
    } else {
        std::path::Path::new(&home)
            .join(rest)
            .to_string_lossy()
            .into_owned()
    }
}
```

Note: this requires that the template has `{home}` at the start (which is the only
sensible usage). A more general approach would parse `template` as a `Path` after
substitution and canonicalise separators, but the simple fix above covers all known
usages.

---

## 3. `start_mount()` Windows desktop — `&PathBuf::from(&mountpoint)` — MINOR STYLE NOTE

**Caller (main.rs ~784):**
```rust
&std::path::PathBuf::from(&mountpoint),
```

**Callee (`CfMountHandle::mount`) signature:**
```rust
pub fn mount(..., mount_path: &Path, ...) -> cloudmount_core::Result<Self>
```

`&PathBuf` coerces to `&Path` via `Deref`, so this is semantically identical to the
previous `std::path::Path::new(&mountpoint)`. Both produce a `&Path` referencing the
same underlying bytes. The `PathBuf::from` constructor normalises separators on Windows
(it converts `/` to `\`), which is a genuine improvement for correctness if `mountpoint`
came from a TOML value with forward slashes.

**One concern:** `mountpoint` is the result of `expand_mount_point`, which — for the
`~/` branch — now already returns a native-separator string via `Path::join`. Creating
a second `PathBuf::from` is redundant but harmless. No bug.

**Clippy:** `&std::path::PathBuf::from(&mountpoint)` creates a temporary `PathBuf`
whose reference is immediately taken. Clippy `clippy::unnecessary_to_owned` or
`clippy::redundant_allocation` will not fire here (those only apply to
`.to_owned()`/`Arc` patterns). However, `clippy::needless_pass_by_ref_mut` is not
relevant either. The expression is clean under default Clippy settings.

**Verdict:** Semantically correct and an improvement. The style is marginally more
verbose than needed but causes no Clippy failures. No action required.

---

## 4. `run_headless()` Windows warn block — CORRECT, ONE OBSERVATION

```rust
#[cfg(target_os = "windows")]
{
    tracing::warn!(
        "headless mode: CfApi mount for '{}' not started — crash recovery skipped for this mount",
        mount_config.name
    );
    tracing::warn!(
        "headless mode: CfApi mount for '{}' not started — delta sync skipped for this mount",
        mount_config.name
    );
}
```

The cfg gate is correct (`target_os = "windows"` only). The two separate `warn!` lines
replace the previous single vague message with operationally useful statements that
explain exactly what is skipped. This is a pure log-quality improvement.

**Observation:** `mount_entries` on Windows is declared `let` (not `let mut`) because
it is never pushed to on that platform:

```rust
#[cfg(target_os = "windows")]
let mount_entries: Vec<(String, Arc<CacheManager>, Arc<InodeTable>)> = Vec::new();
```

The `for` loop still calls `std::fs::create_dir_all(&mountpoint)` on Windows
(lines 1189–1192) before reaching the Windows warn block. This means the mount
directory is created on disk even though the mount is not started. This is a pre-existing
issue — not introduced by this change — but the new warn messages do not mention it.
Consider adding a `continue` or skipping `create_dir_all` on Windows in headless mode,
or at least noting this in the warn message. Not a blocking issue.

**Verdict:** No bugs introduced by this change.

---

## 5. Cfg Gate Audit

| Location | Gate | Correct? |
|---|---|---|
| `derive_mount_point` | none (pure logic) | Yes — platform-neutral |
| `expand_mount_point` | none (pure logic) | Yes — platform-neutral |
| `start_mount` FUSE | `#[cfg(all(feature = "desktop", any(target_os = "linux", target_os = "macos")))]` | Yes |
| `start_mount` CfApi | `#[cfg(all(feature = "desktop", target_os = "windows"))]` | Yes |
| headless Windows warn | `#[cfg(target_os = "windows")]` | Yes |
| `mount_entries` mut/non-mut | split by `any(linux, macos)` / `windows` | Yes |

No missing or mismatched cfg gates in the changed code.

---

## Summary

| # | Severity | Location | Issue |
|---|---|---|---|
| 1 | Low-medium | `config.rs:353–354` | `{home}` substitution in `expand_mount_point` else-branch does not normalise path separators; mixed-separator output on Windows if template contains `{home}/...` |
| 2 | Info | `main.rs:1189–1192` | `create_dir_all` runs on Windows in headless mode even though the mount is not started (pre-existing, not introduced by this change) |

**All other changes are correct.** The `derive_mount_point` rewrite, the `~/` expansion
fix, the `&PathBuf::from(&mountpoint)` call, and the two-line headless warn split are
all sound and introduce no Clippy issues or cross-platform regressions.
