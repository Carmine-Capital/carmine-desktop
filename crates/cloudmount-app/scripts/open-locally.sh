#!/bin/bash
# CloudMount Nautilus Script: Open Locally
#
# This script opens files from CloudMount mounts with the default local application
# by right-clicking in Nautilus and selecting Scripts > "Open Locally".
#
# INSTALLATION:
# 1. Copy this script to ~/.local/share/nautilus/scripts/
# 2. Make it executable: chmod +x ~/.local/share/nautilus/scripts/Open\ Locally
# 3. Restart Nautilus: nautilus -q
#
# REQUIREMENTS:
# - xdg-utils (for xdg-open) - typically pre-installed on Linux

# Nautilus passes selected file paths via NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
IFS=$'\n'
for path in $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS; do
    if [ -n "$path" ]; then
        xdg-open "$path"
    fi
done
