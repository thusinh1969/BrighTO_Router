# Codex audit — desktop model route action density

Date: 2026-09-18
Scope: Portal Models & Routes desktop viewport.

## Problem found

The Models & Routes desktop card used a narrow action column. Disable, Edit, and Delete stacked vertically, which made each model route row taller than necessary and created too much whitespace. This looked weak for production use when many model routes exist.

## Fix applied

- Kept the existing mobile card layout.
- For desktop viewports at 1200px and wider, reserved enough width for route actions.
- Rendered Disable, Edit, and Delete on one compact row.
- Added a visual audit assertion so desktop route actions cannot silently regress to a vertical stack.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_visual_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_visual_audit`: PASS, 0 failures.
- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_polish_audit`: PASS, 0 failures, 0 console errors.
- `portal_modal_surface_audit`: PASS, 0 failures, 0 console errors.
- `portal_static_gate`: PASS.

Key visual audit evidence:

```text
desktop-1440 action buttons y=347,347,347
bodyScrollWidth=1440 htmlClientWidth=1440
```
