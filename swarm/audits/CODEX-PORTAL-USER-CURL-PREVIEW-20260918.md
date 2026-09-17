# CODEX audit — User Portal cURL quick-start preview

Date: 2026-09-18
Scope: User Portal Dashboard, `Call endpoint` panel.

## Finding
The User Portal had copy buttons for endpoint and cURL, but the full cURL command was not visible. A user could copy it but could not inspect the exact request shape before running it. The first preview attempt also exposed a release risk: inline JavaScript escape errors in `static/index.html` were not covered by the cheap static gate.

## Fix applied
- Added a visible `Ready cURL test` preview to the User Portal call endpoint panel.
- The command uses a `BRIGHTO_ENDPOINT` variable and pretty JSON body so mobile users can read it without horizontal page overflow.
- Kept the `Copy cURL` action as the source of truth for copying the complete command.
- Extended `portal_user_journey_audit.mjs` to require the cURL preview and verify it contains endpoint, auth header, model, and sample prompt.
- Extended `portal_static_gate.py` to parse inline Portal JavaScript with `node --check` when Node is available.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed, including inline JavaScript parsing.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshot inspected: `swarm/out/playwright/20260918-063424-portal-user-journey-audit/mobile-390-user-dashboard.png`.
