---
id: ui-dark-premium-redesign
title: Dark Premium UI Redesign
status: completed
created: 2026-03-08T12:00:00Z
completed_at: 2026-03-08T12:32:30.669Z
---

# Intent: Dark Premium UI Redesign

## Goal

Redesign the entire CloudMount UI (wizard + settings) with a dark, premium, OLED-first aesthetic
using a consistent design system. Replace the generic flat look with a real visual identity,
depth, and polish — inspired by Vercel/Linear.

## Users

End users of CloudMount — professionals on Microsoft 365 who interact with the wizard (first-time
setup) and the settings window (ongoing configuration).

## Problem

The current UI is functionally correct but visually generic and fragmented. Each page styles
independently with no shared tokens. There is no visual personality, depth, or brand identity —
it looks like a default form UI, not a polished desktop app.

## Success Criteria

- Both wizard and settings render with the Violet/Space dark palette (accent `#7c5cfc`, base `#0e0f14`)
- Inter font loaded locally and displayed across all UI surfaces
- A single shared `styles.css` drives all visual tokens (colors, spacing, radius, shadows, typography)
- A shared `ui.js` provides common utilities (e.g., status/toast notifications)
- No duplicated CSS between `wizard.html` and `settings.html`
- The UI feels cohesive — wizard and settings share the same design language
- Pending wizard UX fix absorbed: "Get started" button replaced with "Close" in add-mount wizard mode

## Constraints

- CSP: `style-src 'self' 'unsafe-inline'` — Inter font must be bundled locally in `dist/fonts/`
- No external CDN dependencies (Tauri serves all assets from `dist/`)
- Vanilla HTML/CSS/JS only — no UI framework (existing tech stack)
- Must not break existing Tauri command wiring in `wizard.js` / `settings.js`
- Build must continue to pass — only frontend files in `dist/` change

## Notes

### Palette: Violet / Space

| Token             | Value     | Role                          |
|-------------------|-----------|-------------------------------|
| `--bg-base`       | `#0e0f14` | Page background               |
| `--bg-surface`    | `#16181f` | Cards, panels                 |
| `--bg-elevated`   | `#1e2028` | Modals, dropdowns, inputs     |
| `--border`        | `#2a2d3a` | All borders and dividers      |
| `--accent`        | `#7c5cfc` | Primary CTA, active states    |
| `--accent-hover`  | `#9074ff` | Hover state for accent        |
| `--text-primary`  | `#edeef2` | Headings, primary text        |
| `--text-secondary`| `#8b8fa8` | Labels, captions              |
| `--text-muted`    | `#5c607a` | Placeholders, disabled text   |
| `--success`       | `#22c55e` | Success states                |
| `--danger`        | `#f04747` | Destructive actions, errors   |

### Font

Inter (self-hosted in `dist/fonts/`) — Variable font if available, otherwise 400/500/600 weights.
