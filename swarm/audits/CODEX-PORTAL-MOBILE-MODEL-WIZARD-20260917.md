# CODEX audit — Mobile Add model wizard density

Date: 2026-09-17

## Verdict

Fixed a mobile Add model density issue.

The Add model modal is the most important admin flow, but on mobile its three wizard steps were stacked vertically. They consumed too much of the viewport before the admin reached Provider, URL, API key, and model selection.

## Change

- `static/index.html`
  - Mobile Add model wizard steps now render as three compact chips on one row.
  - Reduced mobile wizard chip padding/font slightly while preserving the step labels and guidance.
- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added a mobile gate requiring the three Add model wizard steps to stay on one row.
  - Added a guard against cramped chips.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_modal_surface_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - Log: `swarm/out/portal_modal_surface_audit-mobile-model-wizard-235412.log`

## Screenshot evidence

- `swarm/out/playwright/20260917-235412-portal-modal-surface-audit/mobile-add-model.png`
