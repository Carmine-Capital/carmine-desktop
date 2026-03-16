#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"
TAURI_CONF="$REPO_ROOT/crates/carminedesktop-app/tauri.conf.json"

UPLOAD_HOST="static.carminecapital.com"
UPLOAD_PATH="/var/www/users/carminec/carmine-desktop"

current_version=$(jq -r '.version' "$TAURI_CONF")

# --- Usage ---
if [ $# -lt 1 ]; then
    echo "Usage: $0 <version> [--upload-only]"
    echo ""
    echo "  Current version: $current_version"
    echo "  Example: $0 0.2.0"
    echo "  Example: $0 0.2.0-rc.1"
    echo "  Example: $0 0.2.0 --upload-only   (skip version bump, just upload)"
    exit 1
fi

new_version="$1"
tag="v$new_version"
upload_only=false

if [ "${2:-}" = "--upload-only" ]; then
    upload_only=true
fi

if [ "$upload_only" = true ]; then
    echo "=== Upload Only Mode ==="
    echo ""
    echo "Uploading local build artifacts to $UPLOAD_HOST..."

    ARTIFACTS_DIR="$REPO_ROOT/target/release/bundle"
    if [ ! -d "$ARTIFACTS_DIR" ]; then
        echo "ERROR: No build artifacts found at $ARTIFACTS_DIR"
        echo "       Run 'cargo tauri build --features desktop' first."
        exit 1
    fi

    # Collect artifacts
    STAGING_DIR=$(mktemp -d)
    trap 'rm -rf "$STAGING_DIR"' EXIT

    find "$ARTIFACTS_DIR" -type f \( \
        -name '*.AppImage' -o -name '*.AppImage.tar.gz' -o -name '*.AppImage.tar.gz.sig' \
        -o -name '*.app.tar.gz' -o -name '*.app.tar.gz.sig' -o -name '*.dmg' \
        -o -name '*.exe' -o -name '*.nsis.zip' -o -name '*.nsis.zip.sig' \
        -o -name '*.deb' \
    \) -exec cp {} "$STAGING_DIR/" \;

    echo "Staged artifacts:"
    ls -la "$STAGING_DIR/"

    rsync -avz --chmod=D755,F644 \
        "$STAGING_DIR/" \
        "carminec@${UPLOAD_HOST}:${UPLOAD_PATH}/"

    echo ""
    echo "Done. Artifacts uploaded to https://${UPLOAD_HOST}/carmine-desktop/"
    exit 0
fi

# --- Preflight checks ---
if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: Working tree is dirty. Commit or stash changes first."
    exit 1
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "ERROR: Tag $tag already exists."
    exit 1
fi

# --- Summary & confirmation ---
echo "=== Release ==="
echo ""
echo "  Current version : $current_version"
echo "  New version     : $new_version"
echo "  Tag             : $tag"
echo "  Branch          : $(git branch --show-current)"
echo "  Upload target   : https://${UPLOAD_HOST}/carmine-desktop/"
echo ""
echo "This will:"
echo "  1. Update version in Cargo.toml and tauri.conf.json"
echo "  2. Commit the version bump"
echo "  3. Create tag $tag"
echo "  4. Push commit and tag to origin (triggers release workflow)"
echo "  5. Release workflow builds + uploads to $UPLOAD_HOST"
echo ""
read -rp "Proceed? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# --- Update versions ---
sed -i "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_TOML"
jq --arg v "$new_version" '.version = $v' "$TAURI_CONF" > "$TAURI_CONF.tmp" && mv "$TAURI_CONF.tmp" "$TAURI_CONF"

# --- Commit, tag, push ---
git add "$CARGO_TOML" "$TAURI_CONF"
git commit -m "Bump version to $new_version"
git tag "$tag"
git push origin "$(git branch --show-current)" "$tag"

echo ""
echo "Done. Release workflow triggered for $tag."
echo "Watch it with: gh run list --limit 1"
echo "Artifacts will be uploaded to: https://${UPLOAD_HOST}/carmine-desktop/"
