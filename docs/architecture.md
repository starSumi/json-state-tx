# Architecture and Guarantees

`json-state-tx` is a conditional transaction protocol for one local JSON file. It is not a database and does not provide multi-file ACID transactions.

## Transaction

1. Open the permanent `<state>.lock` sidecar and acquire its exclusive advisory lock.
2. Read the exact state bytes while holding the lock.
3. Compare `sha256:<hex>` against the caller's `--if-version` precondition.
4. Parse and clone the document, apply the complete RFC 6902 patch in memory, and validate the complete candidate against an optional Draft 2020-12 JSON Schema.
5. Serialize the candidate as UTF-8 pretty JSON with LF and a trailing LF.
6. Write and sync a temporary file in the destination directory, then atomically replace the destination.
7. Release the sidecar lock.

The successful OS replacement is the transaction's linearization point. Errors before publish leave the old target untouched. A publish error is conservatively reported as `COMMIT_OUTCOME_UNKNOWN` because some OS and network-filesystem errors can occur after partial progress; callers must read current state before deciding whether to retry.

## Boundaries

- Locks are cooperative. A process that ignores the sidecar protocol can race with this tool.
- Versions hash exact bytes, not semantic JSON. Reformatting a file changes its version.
- Unix commits sync the temporary file and parent directory through `atomic-write-file`.
- Windows flushes candidate content and performs a replace, but Windows has no portable parent-directory `fsync` equivalent. The response reports `content_flushed_metadata_best_effort`.
- Local filesystems are the supported deployment target. Network filesystems may weaken locking, replacement, and error-reporting semantics.
- Process-kill tests demonstrate absence of partial JSON, not universal power-loss durability.

## Stable Errors

The CLI writes one JSON object to stdout. Success exits `0`. Version conflict exits `20`, lock timeout `21`, invalid patch/document/schema `30`, ordinary I/O `40`, and unknown commit outcome `41`.

The `error.code`, `error.phase`, `error.committed`, and `error.os_error` fields are intended for scripts and agents. Human-readable messages are not a stable API.

