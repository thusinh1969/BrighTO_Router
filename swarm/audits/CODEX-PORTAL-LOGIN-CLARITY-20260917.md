# CODEX audit — Login clarity

Date: 2026-09-17
Scope: Portal login screen polish.

## Finding

The login screen looked clean, but the Admin/User switch was not explicit enough:

- Admin mode showed a generic `Password` label.
- User mode exposed a separate API key field, but the main copy did not clearly say that users paste a visible `lc-...` client API key from the API Keys screen.
- Existing portal audits focused mostly on authenticated screens, so this could regress without a targeted gate.

## Fix applied

- Admin login now says: `Admin signs in with the master key from .env.`
- Admin password label is now `Admin key` with `ADMIN_MASTER_KEY` placeholder.
- User login now says: `User signs in with a client API key.`
- User helper text says to paste the visible `lc-...` key issued from the API Keys screen.
- Added `portal_login_audit.sh` / `portal_login_audit.mjs` to screenshot and verify Admin/User login states on desktop and mobile.

## Verification

Passed locally on the current worktree:

- `python3 swarm/scripts/portal_static_gate.py`
- JavaScript syntax extraction + `node --check /tmp/brighto-portal.js`
- `node --check` for all portal Playwright audit scripts including `portal_login_audit.mjs`
- `git diff --check`
- `bash swarm/scripts/portal_login_audit.sh`
- `bash swarm/scripts/portal_empty_state_audit.sh`
- `bash swarm/scripts/portal_logic_acceptance.sh`
- `bash swarm/scripts/portal_visual_audit.sh`
- `bash swarm/scripts/portal_polish_audit.sh`
- `bash swarm/scripts/portal_full_page_audit.sh`
- `bash swarm/scripts/portal_modal_surface_audit.sh`
- `bash swarm/scripts/portal_user_journey_audit.sh`

Evidence logs:

- `swarm/out/portal_login_audit-210227.log`
- `swarm/out/portal_login_audit-login-copy-full-210246.log`
- `swarm/out/portal_empty_state_audit-login-copy-full-210250.log`
- `swarm/out/portal_logic_acceptance-login-copy-full-210255.log`
- `swarm/out/portal_visual_audit-login-copy-full-210310.log`
- `swarm/out/portal_polish_audit-login-copy-full-210319.log`
- `swarm/out/portal_full_page_audit-login-copy-full-210327.log`
- `swarm/out/portal_modal_surface_audit-login-copy-full-210346.log`
- `swarm/out/portal_user_journey_audit-login-copy-full-210358.log`
