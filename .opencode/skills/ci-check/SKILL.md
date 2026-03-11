---
name: ci-check
description: Run the full CI check suite locally (fmt, clippy, build, test) matching what GitHub Actions enforces.
disable-model-invocation: true
---

Run the following checks in order using `toolbox run -c cloudmount-build`, stopping on first failure. Match CI exactly with `RUSTFLAGS=-Dwarnings`.

1. `toolbox run -c cloudmount-build cargo fmt --all -- --check` — formatting
2. `toolbox run -c cloudmount-build cargo clippy --all-targets` — core lints
3. `toolbox run -c cloudmount-build cargo clippy --all-targets --features desktop` — desktop lints
4. `toolbox run -c cloudmount-build cargo build --all-targets` — build
5. `toolbox run -c cloudmount-build cargo test --all-targets` — tests

Set `RUSTFLAGS=-Dwarnings` for steps 2 and 3. Report pass/fail for each step with the exact error output on failure.
