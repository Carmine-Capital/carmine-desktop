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
        -name '*.exe' -o -name '*.exe.sig' \
        -o -name '*.nsis.zip' -o -name '*.nsis.zip.sig' \
    \) -exec cp {} "$STAGING_DIR/" \;

    echo "Staged artifacts:"
    ls -la "$STAGING_DIR/"

    # --- Generate latest.json for Tauri updater ---
    BASE_URL="https://${UPLOAD_HOST}/carmine-desktop"
    PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    read_sig() {
        local sig_file="$1"
        if [ -f "$sig_file" ]; then cat "$sig_file"; else echo ""; fi
    }

    # Detect Windows updater bundle. Prefer .nsis.zip over raw setup.exe.
    WIN_BUNDLE=$(find "$STAGING_DIR" -maxdepth 1 -name '*.nsis.zip' ! -name '*.sig' | head -1)
    if [ -z "$WIN_BUNDLE" ]; then
        WIN_BUNDLE=$(find "$STAGING_DIR" -maxdepth 1 -name '*-setup.exe' | head -1)
    fi
    WIN_SIG=$(read_sig "${WIN_BUNDLE}.sig" 2>/dev/null || echo "")
    WIN_FILENAME=$(basename "$WIN_BUNDLE" 2>/dev/null || echo "")

    if [ -z "$WIN_SIG" ] || [ -z "$WIN_FILENAME" ]; then
        echo ""
        echo "ERROR: No signed Windows updater bundle found in $STAGING_DIR"
        echo "       Expected *.nsis.zip + .sig or *-setup.exe + .sig"
        exit 1
    fi

    jq -n \
        --arg version "$new_version" \
        --arg notes "Release v${new_version}" \
        --arg pub_date "$PUB_DATE" \
        --arg win_sig "$WIN_SIG" \
        --arg win_url "${BASE_URL}/${WIN_FILENAME}" \
        '{
          version: $version,
          notes: $notes,
          pub_date: $pub_date,
          platforms: {
            "windows-x86_64": {
              signature: $win_sig,
              url: $win_url
            }
          }
        }' > "$STAGING_DIR/latest.json"

    echo ""
    echo "=== latest.json ==="
    cat "$STAGING_DIR/latest.json"

    # Upload artifacts + manifest
    rsync -avz --chmod=D755,F644 \
        "$STAGING_DIR/" \
        "carminec@${UPLOAD_HOST}:${UPLOAD_PATH}/"

    echo ""
    echo "Done. Artifacts uploaded to https://${UPLOAD_HOST}/carmine-desktop/"
    echo "Updater manifest: https://${UPLOAD_HOST}/carmine-desktop/latest.json"
    exit 0
fi

# --- Preflight checks ---
current_branch=$(git branch --show-current)
if [ "$current_branch" != "main" ]; then
    echo "ERROR: Releases must be created from the main branch."
    echo "       Current branch: $current_branch"
    exit 1
fi

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
echo "  2. Regenerate Cargo.lock"
echo "  3. Commit the version bump (Cargo.toml, tauri.conf.json, Cargo.lock)"
echo "  4. Create tag $tag"
echo "  5. Push commit and tag to origin (triggers release workflow)"
echo "  6. Release workflow builds + uploads to $UPLOAD_HOST"
echo ""
read -rp "Proceed? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# --- Update versions ---
sed "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_TOML" > "$CARGO_TOML.tmp" && mv "$CARGO_TOML.tmp" "$CARGO_TOML"
jq --arg v "$new_version" '.version = $v' "$TAURI_CONF" > "$TAURI_CONF.tmp" && mv "$TAURI_CONF.tmp" "$TAURI_CONF"

# --- Verify substitutions ---
if ! grep -q "^version = \"$new_version\"" "$CARGO_TOML"; then
    echo "ERROR: Failed to update version in Cargo.toml (expected version = \"$new_version\")"
    git checkout -- "$CARGO_TOML" "$TAURI_CONF"
    exit 1
fi
conf_version=$(jq -r '.version' "$TAURI_CONF")
if [ "$conf_version" != "$new_version" ]; then
    echo "ERROR: Failed to update version in tauri.conf.json (got $conf_version, expected $new_version)"
    git checkout -- "$CARGO_TOML" "$TAURI_CONF"
    exit 1
fi

# --- Regenerate Cargo.lock ---
echo "Regenerating Cargo.lock..."
cargo generate-lockfile --quiet

# --- Commit, tag, push ---
git add "$CARGO_TOML" "$TAURI_CONF" "$REPO_ROOT/Cargo.lock"
git commit -m "Bump version to $new_version"
git tag "$tag"
git push origin "$(git branch --show-current)" "$tag"

echo ""
echo "Done. Release workflow triggered for $tag."
echo "Watch it with: gh run list --limit 1"
echo "Artifacts will be uploaded to: https://${UPLOAD_HOST}/carmine-desktop/"
