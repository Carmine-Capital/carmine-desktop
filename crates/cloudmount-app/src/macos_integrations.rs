//! macOS Finder context menu integration (placeholder).
//!
//! On macOS, Finder context menu entries require either a Finder Sync Extension
//! (bundled as an appex inside the .app bundle) or an Automator Quick Action /
//! Services menu entry. Both approaches need Xcode project scaffolding that is
//! not yet implemented.
//!
//! For now, the `open_online` Tauri command works on macOS via `open::that()`,
//! and the `cloudmount://open-online` deep-link is registered through the
//! Info.plist `CFBundleURLTypes` entry handled by tauri-plugin-deep-link.
//!
//! TODO(macos): Implement Finder context menu integration:
//!   - Option A: Finder Sync Extension (appex) with two menu items:
//!       "Open Online (SharePoint)" -- dispatches cloudmount://open-online?path=...
//!       "Open Locally" -- NSWorkspace.shared.open(url)
//!   - Option B: Automator Quick Actions installed to ~/Library/Services/
//!   - Either approach should register on first mount and clean up on last unmount,
//!     mirroring the Linux and Windows integration patterns.
