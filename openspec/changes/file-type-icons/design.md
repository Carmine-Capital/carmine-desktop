## Context

When Carmine Desktop registers file associations on Windows (`shell_integration.rs:53-121`), it creates ProgID keys like `CarmineDesktop.OfficeFile.docx` with a `shell\open\command` pointing to the executable, but no `DefaultIcon` subkey. Windows falls back to the application's main icon (resource index 0) for all associated file types, making `.docx`, `.xlsx`, and `.pptx` files visually indistinguishable.

Custom SVG icons already exist at `crates/carminedesktop-app/icons/files/` (doc, xls, ppt, pdf) with Office-appropriate color coding. These need to be converted to `.ico`, embedded in the executable as Win32 icon resources, and referenced in the registry during association registration.

The build system already has `embed-resource` in the dependency tree (`tauri-build` → `tauri-winres` → `embed-resource`).

## Goals / Non-Goals

**Goals:**
- Each Office file type displays its own distinct icon when Carmine Desktop is the registered handler
- Icons are self-contained in the executable (no external `.ico` files at runtime)
- No visual disruption to existing app icon or navigation pane icon
- Clean unregistration continues to work (icon references removed with ProgID deletion)

**Non-Goals:**
- macOS document type icons (different mechanism via `Info.plist` / `CFBundleDocumentTypes` — future work)
- Linux file type icons (file associations are no-op stubs on Linux)
- Icon overlay handlers / shell extensions for sync status badges on mounted files
- Runtime icon selection based on Office installation (Approach A from exploration — rejected in favor of self-contained icons)

## Decisions

### 1. Embed icons via `embed-resource` with a `.rc` resource script

**Choice**: Add a `file_icons.rc` resource script with ordinals 101–104, compiled by `embed_resource::compile()` in `build.rs` before `tauri_build::build()`.

**Alternatives considered**:
- *Use `tauri-winres` API directly*: `tauri_build::build()` handles the `WindowsResource` object internally — no access to add extra icons to it.
- *Ship `.ico` files alongside the exe*: Works but fragile (files can be deleted, path assumptions break on updates).
- *Copy icons from Office installation at registration time*: Depends on Office being installed and icon paths being stable. Breaks for users with only Office Online.

**Rationale**: `embed-resource` is already in the dependency tree. Separate `.rc` file with high ordinals (101+) avoids any conflict with Tauri's app icon at ordinal 1. Self-contained in the binary.

### 2. Use negative ordinal syntax (`-N`) in DefaultIcon registry values

**Choice**: Reference icons as `"C:\...\CarmineDesktop.exe,-101"` rather than `"...,1"`.

**Rationale**: The `,N` (positive) syntax is a 0-based enumeration index across all icon groups — fragile if Tauri changes how it embeds the app icon. The `,-N` (negative) syntax directly addresses the resource ordinal, which we control via the `.rc` file. Unambiguous regardless of resource ordering.

### 3. Offline SVG → ICO conversion (committed artifacts)

**Choice**: Convert SVGs to `.ico` files offline and commit them to the repository.

**Alternatives considered**:
- *Build-time conversion*: Would require an SVG rasterizer as a build dependency (resvg, imagemagick). Adds CI complexity.

**Rationale**: Icons change rarely. Committing `.ico` files keeps the build simple and reproducible.

### 4. Extension-to-ordinal mapping as a const table

**Choice**: A `const ICON_ORDINALS: &[(&str, u16)]` table in `shell_integration.rs` maps each extension to its resource ordinal.

**Rationale**: Simple, compile-time checked, co-located with the registration logic. Easy to extend when new file types are added (e.g., PDF).

## Risks / Trade-offs

- **[Ordinal collision with Tauri]** → Mitigated by using ordinals 101+ (Tauri uses ordinal 1 for the app icon). The negative ordinal syntax eliminates index-based ambiguity entirely.
- **[ICO files increase exe size]** → Each multi-resolution `.ico` is ~50-100KB. Four icons add ~200-400KB. Negligible for a desktop app.
- **[Icon cache staleness]** → Windows caches file type icons aggressively. After updating icons in a new version, users may see old icons until the cache refreshes. `SHChangeNotify(SHCNE_ASSOCCHANGED)` is already called during registration, which forces a cache flush.
- **[embed-resource build on non-Windows]** → `embed_resource::compile()` returns `CompilationResult::NotWindows` on non-Windows targets. Using `.manifest_optional().unwrap()` handles this gracefully.
