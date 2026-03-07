# Spec Notes

No capability spec changes required.

This change fixes a subprocess spawning implementation detail — the user-visible behavior
(clicking a tray mount item opens the folder in the file manager) is unchanged. The fix
ensures that behavior actually works reliably on AppImage deployments by stripping
AppImage-injected environment variables before spawning child processes.

No `openspec/specs/` files are modified or created by this change.
