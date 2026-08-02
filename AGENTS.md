# Repository Instructions

## Scope

Keep this repository focused on conditional transactions for one local JSON file. Do not add a daemon, MCP server, general document editor, multi-file transaction layer, or registry-specific business schema without a separate public-value decision.

## Stable Contract

Treat the JSON envelope, error codes, exit codes, `sha256:<hex>` version format, RFC 6902 input, and permanent sidecar-lock protocol as compatibility surfaces. Any change requires regression tests and an update to `docs/architecture.md`.

## Required Checks

Run `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-features`, and `cargo doc --locked --no-deps` before proposing a change. Dependency changes must also pass `cargo deny check bans licenses sources` and `cargo audit`.

## Publication Gate

Public documentation, code comments, errors, issues, and pull requests are English. Explain observable user value and why existing crates do not already solve it. Do not claim SOTA, full ACID, universal crash safety, or production readiness. Do not remove `publish = false` except in a dedicated release-readiness pull request.

