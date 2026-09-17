# Codex audit — Role-aware Portal topbar descriptions — 2026-09-17

## What changed

The Portal topbar description now changes by role.

Admin mode keeps the admin descriptions:

- Dashboard: `Gateway overview`
- Usage: `Usage and requests`
- Settings: `Runtime configuration`

User mode now shows client-facing descriptions:

- Dashboard: `Your API access and usage`
- Usage: `Your requests and token usage`
- Settings: `Portal preferences and session`

## Why

After Settings became visible and useful in User Portal, the topbar still said `Runtime configuration`, which made the user screen look like an unfinished admin page. The page body was already role-aware; the topbar needed to match.

## Permanent gate updated

`swarm/scripts/portal_user_journey_audit.mjs` now captures `#page-desc` and validates role-aware topbar descriptions on desktop and mobile for Dashboard, Usage, and Settings.

## Verification

Syntax/static:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `node --check swarm/scripts/portal_user_journey_audit.mjs` — PASS
- `git diff --check` — PASS

Browser gates:

- `swarm/scripts/portal_empty_state_audit.sh` — PASS
- `swarm/scripts/portal_logic_acceptance.sh` — PASS
- `swarm/scripts/portal_visual_audit.sh` — PASS
- `swarm/scripts/portal_polish_audit.sh` — PASS
- `swarm/scripts/portal_full_page_audit.sh` — PASS
- `swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `swarm/scripts/portal_user_journey_audit.sh` — PASS

Representative logs:

- `swarm/out/portal_user_journey_audit-role-desc-rerun-203212.log`
- `swarm/out/portal_user_journey_audit-role-desc-full-203339.log`
- `swarm/out/portal_full_page_audit-role-desc-full-203307.log`
- `swarm/out/portal_modal_surface_audit-role-desc-full-203327.log`
