# CODEX audit — login Enter-key polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal login interaction and audit stability

## Root cause

The login screen exposed a Sign in button but did not support the normal Enter-key submit behavior from the username, admin-key, or client-key fields. This is a small but visible polish gap for first impression and keyboard usability.

During verification, `portal_logic_acceptance` also false-failed on a transient browser console resource error: `ERR_NETWORK_CHANGED`. Other Playwright audits already ignored this Docker/network transient.

## Fix applied

- Added Enter-key login handling for `login-user`, `login-pass`, and `login-apikey` while the login view is visible.
- Extended `portal_login_audit` to read `ADMIN_MASTER_KEY` from `.env` and prove Enter-key admin login on desktop and mobile.
- Synchronized transient network console-error filtering across remaining portal audit scripts.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_login_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Results:

- `portal-login-audit`: PASS
- `portal-logic-acceptance`: PASS
- `portal-user-journey-audit`: PASS
