#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"
TAURI_CONF="$REPO_ROOT/crates/cloudmount-app/tauri.conf.json"

current_version=$(jq -r '.version' "$TAURI_CONF")

# --- Usage ---
if [ $# -ne 1 ]; then
    echo "Usage: $0 <version>"
    echo ""
    echo "  Current version: $current_version"
    echo "  Example: $0 0.2.0"
    echo "  Example: $0 0.2.0-rc.1"
    exit 1
fi

new_version="$1"
tag="v$new_version"

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
echo ""
echo "This will:"
echo "  1. Update version in Cargo.toml and tauri.conf.json"
echo "  2. Commit the version bump"
echo "  3. Create tag $tag"
echo "  4. Push commit and tag to origin (triggers release workflow)"
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
