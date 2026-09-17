# CODEX audit — Portal session reload regression gate

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Admin/User Portal session persistence after browser refresh

## Context

A previous UX complaint was that pressing F5 could send the user back to login. The Portal already stores a verified session in browser localStorage with a 12-hour TTL, but this behavior was not enforced by Playwright gates.

## Fix applied

- Added an Admin reload assertion to `portal_full_page_audit.mjs`.
- Added a User reload assertion to `portal_user_journey_audit.mjs`.
- The checks verify that after browser reload:
  - the login screen remains hidden;
  - the app remains visible;
  - the correct Admin/User mode is restored;
  - User mode does not expose Admin-only navigation.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Results:

- `portal-full-page-audit`: PASS
- `portal-user-journey-audit`: PASS

No runtime code changed; Docker image rebuild is not required for audit-only changes.
