# Security Policy

`json-state-tx` is experimental. Security fixes target the latest GitHub Release and `main`; older `0.x` releases may not receive patches.

Use GitHub private vulnerability reporting for issues involving state corruption, lock bypass, unsafe path handling, schema-validation bypass, or release integrity. If private reporting is unavailable, contact the maintainer through the email in the repository's Git commit metadata without including exploit details in a public issue.

The threat model covers accidental races among cooperating local processes. It does not protect against a malicious local user, privileged process, direct writer that ignores the sidecar lock, compromised filesystem, or network filesystem with weak lock/rename semantics. See `docs/architecture.md` before reporting expected behavior outside this boundary.

No response-time or remediation SLA is promised for this experimental project.

