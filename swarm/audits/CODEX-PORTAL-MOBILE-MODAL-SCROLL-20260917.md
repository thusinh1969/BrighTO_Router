# CODEX Portal mobile modal scroll — 2026-09-17

## Result

PASS. Mobile modals now stay inside the viewport and scroll internally instead of growing past the screen height.

## Problem fixed

The Add Model modal is long on mobile. The previous mobile CSS removed the modal max-height and overflow handling, so screenshots and real use could show only the top of the form while the bottom actions lived far below the viewport.

## Change

- `static/index.html`
  - Mobile modal overlay uses tighter viewport padding.
  - Mobile modal gets `max-height: calc(100dvh - 24px)` and internal scroll.
  - Mobile modal action rows are static at the end of the form, avoiding sticky footer overlap with inputs.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Fails if a mobile modal extends below the viewport.
  - Keeps the existing clipped-content and footer-overlap checks.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_modal_surface_audit.mjs
git diff --check
bash swarm/scripts/portal_modal_surface_audit.sh
```

Responsive regression checks:

```bash
portal_full_page_audit PASS
portal_modal_surface_audit PASS
portal_user_journey_audit PASS
```

Evidence log prefix:

```text
swarm/out/*-mobile-modal-scroll-222812.log
```
