# Codex audit — Add model upstream API key needs visible admin control

Date: 2026-09-18

## Finding

The Add model wizard accepted the provider API key in a password-only field. For an admin setup flow, this made first-time provider configuration easier to mistype and harder to verify. The field also did not expose an explicit Show/Hide control.

## Fix applied

- `static/index.html`
  - Added a compact Show/Hide button beside the Add model provider API key field.
  - The field still defaults to hidden when the modal opens.
  - Clicking Show changes the field to plain text without clearing the typed key.
  - Clicking Hide returns it to password mode without clearing the typed key.
  - Mobile Add model footer was reduced from 46px to 40px after the added control exposed a footer overlap regression.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added modal evidence for the provider API key field.
  - Added desktop/mobile regression gates for Show/Hide behavior.
  - Existing modal overflow gates caught and verified the mobile footer fix.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - desktop Show: `password -> text -> password`, typed value preserved
  - mobile Show: `password -> text -> password`, typed value preserved
  - mobile Add model footer: height `40px`, no important control covered
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` — PASS
