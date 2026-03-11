## Purpose

This spec defines the filesystem watcher that detects local file changes within CfApi sync roots on Windows, using `ReadDirectoryChangesW` with debouncing and thread isolation.

## Requirements


