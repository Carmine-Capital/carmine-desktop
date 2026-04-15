---
name: ci-check
description: Run the full CI check suite locally (fmt, clippy, build, test) matching what GitHub Actions enforces. Requires a Windows host — `winfsp-sys` will not compile on Linux/macOS.
disable-model-invocation: true
---

Run the following checks in order on a Windows host, stopping on first failure. Match CI exactly with `RUSTFLAGS=-Dwarnings`.

1. `cargo fmt --all -- --check` — formatting
2. `cargo clippy --all-targets -- -D warnings` — core lints
3. `cargo clippy --all-targets --features desktop -- -D warnings` — desktop lints
4. `cargo build --all-targets --features desktop` — build
5. `cargo test --all-targets --features desktop` — tests

On Linux/macOS the build fails at `winfsp-sys` regardless — push to a branch and let GitHub Actions run the same checks on `windows-latest`.

Report pass/fail for each step with the exact error output on failure.
