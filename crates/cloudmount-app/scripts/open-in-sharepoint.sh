#!/bin/bash
# CloudMount Nautilus Script: Open in SharePoint
# 
# This script allows you to open files from CloudMount mounts in SharePoint/Office Online
# by right-clicking in Nautilus and selecting Scripts > "Open in SharePoint".
#
# INSTALLATION:
# 1. Copy this script to ~/.local/share/nautilus/scripts/
# 2. Make it executable: chmod +x ~/.local/share/nautilus/scripts/Open\ in\ SharePoint
# 3. Restart Nautilus: nautilus -q
#
# REQUIREMENTS:
# - CloudMount must be running (the deep-link protocol handler receives the request)
# - xdg-utils (for xdg-open) - typically pre-installed on Linux

# Nautilus passes selected file paths via NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
IFS=$'\n'
for path in $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS; do
    if [ -n "$path" ]; then
        # Percent-encode the path for use in the deep-link URL
        encoded_path=$(python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.stdin.read().strip(), safe=''))" <<< "$path")
        
        # Invoke the deep-link protocol handler
        xdg-open "cloudmount://open-online?path=${encoded_path}"
    fi
done
