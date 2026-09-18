# Codex audit — Providers page actions should stay on one desktop row

Date: 2026-09-18

## Finding

After the Providers page was corrected to show both `Add model` and `Advanced connection`, the existing CSS still assumed only two controls. On desktop, this could push the filter select to a second row and make the panel header look unfinished.

## Fix applied

- `static/index.html`
  - Split `.models-actions` and `.providers-actions` grid definitions.
  - Providers actions now use three columns on desktop: `Add model`, `Advanced connection`, and filter select.
  - Existing mobile CSS still stacks actions full-width.

- `swarm/scripts/portal_full_page_audit.mjs`
  - Added geometry evidence for Providers page actions.
  - Added a desktop regression gate that verifies all three controls share the same vertical center, which correctly handles the taller select element.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
  - Desktop action centers: `257, 257, 257`.
  - Buttons and select fit on one row.
