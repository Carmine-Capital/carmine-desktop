# Milestones

## v1.0 Stabilization & Observability (Shipped: 2026-03-19)

**Phases completed:** 4 phases, 10 plans, 19 tasks
**Timeline:** 2 days (2026-03-18 → 2026-03-19)
**Lines changed:** +17,936 / -346 across 90 files
**Codebase:** 26,187 LOC Rust + 2,920 LOC JS/HTML/CSS

**Delivered:** App is deployment-ready for Windows org rollout — offline pin crash fixed, full dashboard with sync observability, and modernized UI.

**Key accomplishments:**
1. Fixed WinFsp offline pin crash — 5s VFS-path timeout, memory cache eviction protection, SQLite metadata during pin
2. Built observability infrastructure — ObsEvent bus, ring buffers, 4 Tauri dashboard commands verified end-to-end
3. Delivered dashboard UI — 6-section dashboard with real-time updates via obs-event listener and 30s periodic refresh
4. Modernized visual design — CSS design token refresh (soft dark palette, consolidated typography, normalized spacing)
5. Fixed offline pin health — corrected inode chain for VFS-browsed items, added health badges per pin

### Known Gaps
- **UI-02**: All user-facing actions provide visible feedback — partially complete. Most actions have feedback via `showStatus()`, but visual verification checkpoint was not completed by user.

---

