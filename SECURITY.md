# Security Policy

BrighTO-Router handles API keys, backend provider credentials, usage metadata, and administrative operations. Please do not disclose security issues publicly before maintainers have had a chance to assess them.

## Reporting a vulnerability

Open a private security advisory on GitHub if available. If private advisories are not enabled for the repository yet, contact the maintainers through the project owner channel and include:

- Affected commit or release.
- Clear reproduction steps.
- Expected and observed behavior.
- Impact assessment.
- Any suggested fix.

Do not include real provider keys, customer prompts, production logs, or private traffic captures in the report.

## Security expectations

- Admin API access requires `x-admin-key` and source IP/CIDR allowlisting.
- Client API keys are hashed before storage.
- Backend provider keys are resolved from environment variables or files, not stored as plaintext in route config.
- Usage ledger and logs must not store prompt or message payloads.
- Production deployments should terminate TLS at a hardened edge proxy or load balancer unless binary-level TLS is explicitly configured and tested.
