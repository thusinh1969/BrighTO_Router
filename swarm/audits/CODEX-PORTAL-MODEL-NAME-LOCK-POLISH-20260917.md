# CODEX audit — model route readability and locked delete polish

Date: 2026-09-17
Role: auditor/implementer for Portal polish

## Verdict

PASS after implementation, screenshot review, and Playwright gates.

## Change

The Models & Routes table now treats the public model name as a first-class identifier:

- Public model names wrap fully instead of being clipped with an ellipsis.
- The copy action is aligned in a stable two-column grid beside the name.
- Long model names remain readable on desktop and mobile.

Disabled destructive actions now look disabled instead of dangerous/clickable:

- Locked `Delete` buttons are muted, neutral, and keep the explanatory tooltip.
- Active destructive actions still use the red danger style.

## Regression gates added

`portal_full_page_audit.mjs` now asserts:

- Disabled `.btn.danger` buttons must not render with red danger color/border.
- Model name rows must use a stable grid with one copy button.
- Public model names must not be clipped by ellipsis, hidden overflow, or line clamp.

## Verification

Static/parser:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JS `node --check` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS

Full Portal suite:

- `portal_login_audit` — PASS
- `portal_empty_state_audit` — PASS
- `portal_logic_acceptance` — PASS
- `portal_visual_audit` — PASS
- `portal_polish_audit` — PASS
- `portal_full_page_audit` — PASS
- `portal_modal_surface_audit` — PASS
- `portal_user_journey_audit` — PASS

Primary evidence log prefix: `swarm/out/*-model-name-lock-polish-220041.log`.
