# Local developer shortcuts. CI has its own recipe in .github/workflows/.

FRONTEND_DIR := crates/carminedesktop-app/frontend
APP_DIR      := crates/carminedesktop-app

.PHONY: help run-desktop build-desktop install-frontend build-frontend fmt lint test clean-frontend

help:
	@echo "Available targets:"
	@echo "  run-desktop       Start the desktop app in dev mode (Vite + Tauri)"
	@echo "  build-desktop     Build the release installer locally (Windows NSIS)"
	@echo "  install-frontend  npm ci inside $(FRONTEND_DIR)"
	@echo "  build-frontend    Build the frontend to $(FRONTEND_DIR)/dist"
	@echo "  fmt               cargo fmt --all"
	@echo "  lint              cargo clippy --all-targets --features desktop"
	@echo "  test              cargo test --all-targets"
	@echo "  clean-frontend    Remove node_modules and dist"

# Only re-run `npm ci` when the lockfile changes — avoids the ~30s penalty on
# every invocation of run-desktop.
$(FRONTEND_DIR)/node_modules: $(FRONTEND_DIR)/package-lock.json
	cd $(FRONTEND_DIR) && npm ci
	@touch $@

install-frontend: $(FRONTEND_DIR)/node_modules

build-frontend: $(FRONTEND_DIR)/node_modules
	cd $(FRONTEND_DIR) && npm run build

# Equivalent to the old `cargo run --features desktop` muscle-memory, now that
# the frontend needs a Vite dev server running on :5174 at the same time.
# `cargo tauri dev` auto-spawns it via tauri.conf.json's beforeDevCommand.
run-desktop: $(FRONTEND_DIR)/node_modules
	cd $(APP_DIR) && cargo tauri dev --features desktop

build-desktop: $(FRONTEND_DIR)/node_modules
	cd $(APP_DIR) && cargo tauri build --features desktop

fmt:
	cargo fmt --all

lint: $(FRONTEND_DIR)/node_modules
	cargo clippy --all-targets --features desktop

test:
	cargo test --all-targets

clean-frontend:
	rm -rf $(FRONTEND_DIR)/node_modules $(FRONTEND_DIR)/dist
