# CODEX audit — login state polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal login state and first-impression UX

## Root cause

The login screen supported button and Enter-key sign-in, but state handling was still rough:

- stale Admin login errors remained visible after switching to the User tab;
- Sign in did not enter a disabled/loading state while credentials were being verified;
- repeated Enter/click submits could start duplicate login attempts.

## Fix applied

- Added `loginBusy` state and disabled the Sign in button during credential verification.
- Button text changes to `Signing in…` while the request is in progress.
- Added shared `clearLoginError` / `showLoginError` helpers.
- Switching Admin/User tabs now clears stale login errors.
- Logout resets login busy/error state.
- Extended `portal_login_audit` to prove invalid-login recovery and stale-error clearing on desktop and mobile.

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
