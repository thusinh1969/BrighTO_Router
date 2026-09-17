# CODEX Portal Add model modal polish — 2026-09-18

## Finding

The Add model flow is the most important admin path. The previous modal passed basic logic, but the first screen still had two UX defects:

1. The model mapping preview showed fake technical placeholders (`public-model -> provider-model`) before the admin had selected anything.
2. The route modal action footer could visually cover the mapping preview or the Optional limits control on small screens.

Both issues make the flow feel less precise than the underlying route/test/save logic.

## Change

- `static/index.html`
  - Added an inline BrighTO favicon so the Portal does not generate browser 404 noise for a missing icon.
  - Grouped Provider, Base URL, and API key into a desktop provider-connection grid; mobile stays one column.
  - Replaced fake mapping placeholders with clear empty-state copy: `Public name` and `Provider model`.
  - Added a compact mobile mapping sentence so the Add model modal stays readable on a phone.
  - Made desktop route modal actions sticky without covering the previous control.
  - Tuned mobile route modal spacing, max height, and initial scroll so Test/Save actions remain visible without covering important controls.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Fails if Add model preview regresses to fake `public-model/provider-model` placeholder values.
  - Fails if the action footer covers either the model mapping preview or Optional limits control.

## Verification

Runtime target:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443
```

Checks run:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_modal_surface_audit.mjs
```

Results:

- Modal surface audit: `PASS`, `failures=0`, `consoleErrors=0`
- Full-page audit: `PASS`, `failures=0`, `consoleErrors=0`
- Logic acceptance: `PASS`, `failures=0`, `consoleErrors=0`
- Polish audit: `PASS`, `failures=0`, `consoleErrors=0`
- Static gate: `PORTAL_STATIC_GATE PASS`
- JavaScript syntax check: pass

Artifacts:

```text
swarm/out/playwright/20260918-060639-portal-modal-surface-audit
swarm/out/playwright/20260918-060832-portal-full-page-audit
```

Measured modal state after the fix:

```text
Desktop Add model: footer visible, no covered controls, no clipped content
Mobile Add model: scrollHeight=816, clientHeight=794, footer visible, no covered controls
```

## Status

Ready to embed into the production Docker image after commit/build/push.
