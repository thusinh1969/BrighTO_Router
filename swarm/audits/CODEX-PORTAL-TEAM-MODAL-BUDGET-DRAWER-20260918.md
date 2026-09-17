# CODEX audit — Team modal optional budget drawer

Date: 2026-09-18

## Verdict

Fixed Create team flow density.

A normal team often starts with unlimited budget and only needs a name. Before this change, Create team exposed Budget type, Period, Token amount, and Advanced budget rules by default, making the common path feel heavier than needed.

## Change

- `static/index.html`
  - Create team now shows Team name, Optional team budget, State, and Create first.
  - Budget controls are inside an `Optional team budget` drawer.
  - Edit team opens the drawer automatically when an existing team already has token, money, or per-model budget caps.
  - Save payload behavior is unchanged: unlimited clears budget, token budget still works, advanced budget rules still support money/per-model caps.
- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added gates that New team budget controls are collapsed by default.
  - Added interaction gate that opening the drawer reveals Budget type, Period, Token amount, and Advanced budget rules.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_modal_surface_audit.mjs` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - Log: `swarm/out/portal_modal_surface_audit-team-modal-budget-drawer-000851.log`
- `bash swarm/scripts/portal_logic_acceptance.sh` — PASS
  - Log: `swarm/out/portal_logic_acceptance-team-modal-budget-drawer-000851.log`
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-team-modal-budget-drawer-000851.log`

## Screenshot evidence

- `swarm/out/playwright/20260918-000851-portal-modal-surface-audit/mobile-new-team.png`
