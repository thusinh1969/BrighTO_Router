# CODEX audit — Provider optional load control drawer

Date: 2026-09-18

## Verdict

Fixed Provider pre-registration density.

The normal Provider pre-registration path only needs a preset, name, base URL, and enabled state. Before this change, Weight and Simultaneous calls were visible by default, making the modal look more technical than the main flow requires.

## Change

- `static/index.html`
  - Provider modal now keeps Weight and Simultaneous calls in an `Optional load control` drawer.
  - Drawer is closed by default for new providers.
  - Drawer opens automatically on edit when a provider has non-default weight or a simultaneous-call cap.
  - Save payload behavior is unchanged: default weight remains `1`, default simultaneous calls remains `0`.
- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added gates that Provider load-control fields are collapsed by default and revealed by the drawer.
- `swarm/scripts/portal_polish_audit.mjs`
  - Updated Provider CRUD test to open the drawer before filling load-control fields.
- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`
  - Updated stale Provider field labels and drawer handling for manual provider CRUD instrumentation.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_modal_surface_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_logic_acceptance_instrumented.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - Log: `swarm/out/portal_modal_surface_audit-provider-load-control-drawer-final-001732.log`
- `bash swarm/scripts/portal_polish_audit.sh` — PASS
  - Log: `swarm/out/portal_polish_audit-provider-load-control-drawer-final-001732.log`
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-provider-load-control-drawer-final-001732.log`
- `bash swarm/scripts/portal_logic_acceptance.sh` — PASS
  - Log: `swarm/out/portal_logic_acceptance-provider-load-control-drawer-final-001732.log`

## Screenshot evidence

- `swarm/out/playwright/20260918-001732-portal-modal-surface-audit/mobile-add-provider.png`
