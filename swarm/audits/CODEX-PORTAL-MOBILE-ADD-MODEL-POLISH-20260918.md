# CODEX audit — Mobile Add model modal polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_modal_surface_audit.mjs`

## Root cause

The mobile Add model modal kept the Provider model input and Load models button on one tight row. At 390px width, the input placeholder was visibly clipped. The sticky footer also used four full text labels, causing the Test connection button to wrap awkwardly.

A first 2x2 footer attempt fixed button readability but made the sticky footer overlap the mapping preview / optional limits control. That approach was rejected by the modal audit and was not kept.

## Fix applied

- Mobile Provider model picker now stacks the input and Load models button as full-width controls.
- Mobile route modal footer stays four columns to avoid covering content.
- Mobile footer actions use short visible labels through CSS (`Test`, `Draft`, `Enable`) while preserving full `aria-label` values (`Test connection`, `Save draft`, `Save enabled`) for accessibility and Playwright flows.
- Desktop modal layout remains unchanged.

## Regression gate

`portal_modal_surface_audit.mjs` now captures:

- mobile model picker row geometry;
- footer button clipping metrics.

It fails mobile Add model if the Provider model picker does not stack cleanly or if footer actions clip/wrap their readable labels.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_modal_surface_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Results:

- `PORTAL_STATIC_GATE PASS`
- modal surface Playwright audit: `PASS`, 0 failures, 0 console errors
- full page Playwright audit: `PASS`, 0 failures, 0 console errors
- user journey Playwright audit: `PASS`, 0 failures, 0 console errors

Screenshot inspected manually:

- `swarm/out/playwright/20260918-072820-portal-modal-surface-audit/mobile-add-model.png`
