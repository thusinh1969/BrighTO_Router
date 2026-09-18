# Codex audit — Portal modals need a consistent top-right close action

Date: 2026-09-18

## Finding

The main Portal modals only exposed footer actions such as `Cancel`, `Close`, or `Done`. On mobile, especially in the Add model wizard, users had to look at the sticky footer to dismiss the dialog. A modern Portal dialog should always expose a consistent top-right close affordance.

## Fix applied

- `static/index.html`
  - Added `modalHeader(title, onClose)` helper.
  - Added a top-right `×` close button with `aria-label="Close dialog"` to the main admin modals:
    - Add/Edit model
    - Prepare/Edit provider connection
    - Create/Edit team
    - Create/Edit API key
    - Key created / key reveal
    - Provider model list
    - Model picker overlay
  - Added shared modal titlebar CSS.
  - Compacted the Add model mobile titlebar and optional drawer spacing so the sticky action footer does not cover controls.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added modal evidence for close buttons.
  - Added regression gate requiring exactly one clear close button per audited modal.
  - Added click verification that the close button dismisses the modal.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - `desktop-add-model-close-button` — PASS
  - `desktop-add-provider-close-button` — PASS
  - `desktop-new-team-close-button` — PASS
  - `desktop-new-key-close-button` — PASS
  - `mobile-add-model-close-button` — PASS
  - `mobile-add-provider-close-button` — PASS
  - `mobile-new-team-close-button` — PASS
  - `mobile-new-key-close-button` — PASS
  - Mobile Add model footer did not cover important controls.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` — PASS
