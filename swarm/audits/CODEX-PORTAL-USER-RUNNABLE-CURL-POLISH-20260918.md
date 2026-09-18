# Codex audit — User Portal cURL sample must be runnable immediately

Date: 2026-09-18

## Finding

The User Dashboard showed a cURL preview with `Authorization: Bearer <your API key>` even after the user had signed in with a client API key. That made the quick-start sample incomplete: users still had to manually replace a placeholder before testing the route.

## Fix applied

- `static/index.html`
  - `userCallPanel()` now builds the Auth card and cURL preview from the signed-in client API key already held in the browser session.
  - The cURL copy button now copies a runnable command for the current endpoint, current allowed model, and current key.
  - The fallback placeholder remains only for impossible/no-session rendering.

- `swarm/scripts/portal_user_journey_audit.mjs`
  - Updated dashboard/copy/preview assertions to require `Authorization: Bearer <signed-in key>`.
  - Added regression protection against `<your API key>` returning in the user cURL preview or copy payload.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` — PASS
  - Auth card included the generated audit client key.
  - Copy cURL payload included the generated audit client key.
  - cURL preview included the generated audit client key.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
