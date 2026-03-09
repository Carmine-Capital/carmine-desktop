# All cargo commands run inside the cloudmount-build toolbox container.
# The app itself must run on the host (FUSE mounts are invisible inside toolbox).
# See docs/dev-setup-immutable-linux.md for setup instructions.

TOOLBOX = toolbox run -c cloudmount-build

.PHONY: build test fmt fmt-check clippy run run-desktop help

build: ## Build all targets
	$(TOOLBOX) cargo build --all-targets

build-desktop: ## Build all targets
	$(TOOLBOX) cargo build --all-targets --features desktop

test: ## Run all tests
	$(TOOLBOX) cargo test --all-targets

fmt: ## Format all code
	$(TOOLBOX) cargo fmt --all

fmt-check: ## Check formatting (CI mode)
	$(TOOLBOX) cargo fmt --all -- --check

clippy: ## Lint all targets (warnings = errors)
	$(TOOLBOX) cargo clippy --all-targets --all-features

check: fmt-check clippy test ## Run all CI checks (fmt + clippy + test)

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'
