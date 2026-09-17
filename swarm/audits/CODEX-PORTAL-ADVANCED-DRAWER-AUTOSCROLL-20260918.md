# Codex audit — mobile optional drawer autoscroll

Date: 2026-09-18
Scope: Modal optional drawers, especially Add model on mobile.

## Problem found

On mobile Add model, the `Optional limits and pricing` drawer sits at the bottom of a long form. With sticky Test/Save actions, opening the drawer could leave the newly revealed fields below the action bar. The drawer was technically present, but the first advanced field was not brought into the readable area.

## Fix applied

- Added `scrollAdvancedIntoView(open, panel)`.
- When an optional drawer opens, the containing modal scrolls the drawer into the readable part of the modal.
- Applied to:
  - Provider optional load control.
  - Model route optional limits and pricing.
  - Team optional budget.
  - API key optional limits and budget.
- Extended modal Playwright audit to open Add model optional limits on mobile and verify the first advanced field is above sticky actions.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_modal_surface_audit`: PASS, 0 failures, 0 console errors.
- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_polish_audit`: PASS, 0 failures, 0 bugs, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence from modal audit:

```text
mobile add-model advanced scrollTop=591
footer top=753
first advanced field top=263 bottom=322
```
