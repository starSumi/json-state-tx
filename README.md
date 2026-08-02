# json-state-tx

`json-state-tx` is a small Rust library and CLI for conditional transactions on a single JSON state file. It combines RFC 6902 JSON Patch, a whole-document version precondition, optional Draft 2020-12 JSON Schema validation, a permanent cross-process sidecar lock, and same-directory atomic publication.

This narrow contract prevents cooperating writers from silently losing updates while keeping state readable, diffable, and easy to back up. It does not claim database-grade or multi-file ACID semantics.

## Install

Download a versioned GitHub Release, verify its checksum and GitHub attestation, then run the checked-out installer:

```powershell
pwsh -NoProfile -File .\scripts\install.ps1 -Version v0.1.0 -InstallSkill -AddToUserPath
```

For development from a trusted checkout:

```powershell
cargo install --path . --locked --force
```

## One-minute example

```powershell
'{"counter":0}' | Set-Content -Encoding utf8 document.json
'[{"op":"replace","path":"/counter","value":1}]' | Set-Content -Encoding utf8 patch.json

$created = json-state-tx init --file state.json --document document.json | ConvertFrom-Json
json-state-tx apply --file state.json --patch patch.json --if-version $created.result.version
json-state-tx read --file state.json
```

Every command writes one stable JSON envelope to stdout. An update reads and rechecks the exact bytes only after acquiring `<state>.lock`; a stale writer gets `VERSION_CONFLICT` and leaves the file unchanged.

## Protocol

- Paths inside patches use RFC 6901 JSON Pointer through RFC 6902.
- `--if-version` is `sha256:<64 lowercase hex>` over the exact current file bytes.
- A patch is applied entirely in memory. Any failed operation or schema violation aborts before publication.
- New files use UTF-8 pretty JSON, LF, and one trailing LF.
- The sidecar lock is permanent because the target file identity changes at commit.

See [Architecture and guarantees](docs/architecture.md) for the linearization point, durability model, filesystem limitations, and stable exit codes.

## Public-value boundary

The patch algorithm comes from `json-patch`, atomic publication comes from `atomic-write-file`, and SHA-256 comes from RustCrypto. This project contributes the small conditional transaction protocol, stable machine contract, and cross-platform integration around those proven components.

The initial `0.x` line is experimental. `publish = false` enforces the crates.io gate until cross-platform CI, crash-injection coverage, dependency policy, and public review are complete.

## Development

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
```

Release archives include `SHA256SUMS`, an SPDX JSON SBOM, and GitHub build provenance. Verify an archive with `gh attestation verify <archive> -R starSumi/json-state-tx`.

Licensed under MIT.
