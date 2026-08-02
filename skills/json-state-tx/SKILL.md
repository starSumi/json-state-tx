---
name: json-state-tx
description: Use when an agent must read, initialize, or conditionally update one local JSON state file without silently overwriting a concurrent writer.
---

# JSON State Transactions

Use `json-state-tx` only for a single local JSON state file whose writers follow the same sidecar-lock protocol. First run `json-state-tx --version`; if the command is unavailable, stop and report that the CLI must be installed.

1. Run `json-state-tx read --file <path>` and retain `result.version`.
2. Express the complete mutation as an RFC 6902 patch file.
3. Run `json-state-tx apply --file <path> --patch <patch.json> --if-version <version>`.
4. On `VERSION_CONFLICT`, reread the current document, recompute the intended patch, and ask before overwriting user intent when the changes conflict semantically.
5. On `COMMIT_OUTCOME_UNKNOWN`, reread the state before any retry.

Do not edit the state directly, delete the permanent `.lock` sidecar, or claim safety against noncooperating writers and network filesystems.
