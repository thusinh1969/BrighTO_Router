# CODEX audit — API key table readability

Date: 2026-09-17
Role: auditor/implementer for Portal polish

## Verdict

PASS after screenshot review and Playwright gates.

## Change

The API Keys table now treats revealed client keys as operational identifiers:

- Revealed client keys fit on one readable line on desktop.
- Copy and Reveal actions stay aligned next to the key.
- Mobile keeps the wrapped stacked-card layout.
- Owner/team text wraps instead of clipping.
- Limit labels are stacked vertically so `Requests/min`, `Simultaneous`, and `Budget` do not collide in the narrow limits column.

## Regression gates added

`portal_full_page_audit.mjs` now asserts:

- Revealed key rows include a full key, one copy action, and one Reveal action.
- Revealed keys are not clipped by ellipsis or hidden overflow.
- Desktop revealed keys fit on one line at the audited 1440px viewport.
- API key limit labels do not overflow their visible cells.

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

Primary evidence log prefix: `swarm/out/*-api-key-readability-220958.log`.
