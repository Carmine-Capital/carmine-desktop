# Dev Setup on Immutable Linux (Fedora Silverblue/Aurora/Kinoite)

On immutable distros, build dependencies live in a toolbox container while the app runs on the host. FUSE mounts only work on the host — not inside containers.

## Prerequisites (host)

These should already be present on Fedora Atomic desktops:

- `fuse3` / `fusermount3` — for the OneDrive FUSE mount
- `fuse-libs` (fuse2) — needed if running AppImages

Verify:

```bash
fusermount3 --version
rpm -q fuse3 fuse-libs
```

## Create a build toolbox

```bash
toolbox create carminedesktop-build
toolbox enter carminedesktop-build
```

Install build dependencies inside the toolbox:

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tauri CLI
cargo install tauri-cli --version "^2"

# System build deps (Fedora)
sudo dnf install -y \
  gtk3-devel \
  webkit2gtk4.1-devel \
  libayatana-appindicator-gtk3-devel \
  librsvg2-devel \
  fuse3-devel \
  openssl-devel \
  pkg-config \
  xdg-utils
```

## Build

### Headless (no GUI)

```bash
toolbox run -c carminedesktop-build cargo build -p carminedesktop-app --release
```

Run on the host:

```bash
./target/release/carminedesktop-app --headless \
  --client-id "your-client-id" \
  --tenant-id "your-tenant-id"
```

### Desktop (Tauri GUI)

```bash
toolbox run -c carminedesktop-build cargo build -p carminedesktop-app --release --features desktop
```

Run on the host:

```bash
./target/release/carminedesktop-app
```

### AppImage

AppImage bundles all runtime dependencies — ideal for immutable distros since nothing needs to be layered on the host.

```bash
toolbox run -c carminedesktop-build env \
  APPIMAGE_EXTRACT_AND_RUN=1 \
  NO_STRIP=true \
  cargo tauri build --features desktop --bundles appimage
```

Run on the host:

```bash
./target/release/bundle/appimage/Carmine Desktop_0.1.0_amd64.AppImage
```

The two extra env vars work around toolbox limitations:

| Variable | Why |
|----------|-----|
| `APPIMAGE_EXTRACT_AND_RUN=1` | linuxdeploy is itself an AppImage — FUSE doesn't work inside toolbox, so this extracts it instead |
| `NO_STRIP=true` | linuxdeploy bundles an old `strip` binary that can't handle Fedora's modern ELF format (`.relr.dyn` sections) |

## Important: always run on the host

FUSE mounts created inside a toolbox container are isolated in the container's mount namespace and invisible on the host. The app will appear to work (mount succeeds, no errors) but the directory will be empty.

**Build** inside toolbox, **run** on the host.

## Tests and linting

These run fine inside toolbox (no FUSE needed):

```bash
toolbox run -c carminedesktop-build cargo test --all-targets
toolbox run -c carminedesktop-build cargo clippy --all-targets --all-features
toolbox run -c carminedesktop-build cargo fmt --all -- --check
```
