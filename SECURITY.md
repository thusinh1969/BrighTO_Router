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
- Client API keys are hashed for fast auth lookup.
- Backend provider keys are resolved from environment variables or files, not stored as plaintext in route config.
- Usage ledger and logs must not store prompt or message payloads.
- Production deployments should terminate TLS at a hardened edge proxy or load balancer unless binary-level TLS is explicitly configured and tested.

## Content logging policy

BrighTO-Router stores request metadata for analytics and operations, not conversation content. The PostgreSQL `usage_ledger` records request id, key/team/model/backend identifiers, status, token counts, timing, streaming/client-abort flags, and a short error class. If PostgreSQL is temporarily unavailable, the same event shape is buffered in the local JSONL file configured by `LEDGER_FALLBACK_FILE` and replayed later. It does not store prompts, message arrays, uploaded media, tool payloads, provider response bodies, or model answers.

Provider error bodies are forwarded to the caller but are not persisted. If a deployment needs transcript retention, implement it as an explicit opt-in enterprise feature with encryption, redaction, retention limits, and audited access.

## Admin key re-view

To let administrators view client API keys again after creation, the plaintext is stored in
`api_keys.key_secret` (migration 0003). Only the admin `GET /admin/keys/{id}/reveal` endpoint returns
it; list endpoints and user endpoints never expose it.

This is a deliberate open-source/simple-mode tradeoff chosen by the operator. Protect PostgreSQL
and `ADMIN_MASTER_KEY`. For production/enterprise, prefer encrypting `key_secret` at rest
(`key_ciphertext` + `API_KEY_ENCRYPTION_SECRET`) or a managed secret store.

Provider LLM API keys remain environment/file references and are never returned as plaintext.
