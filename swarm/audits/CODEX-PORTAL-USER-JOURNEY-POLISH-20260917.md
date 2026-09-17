# Codex audit — Portal user journey polish — 2026-09-17

## What changed

User Portal now has a practical `Call endpoint` panel on the Dashboard:

- Method: `POST`
- Base URL: current Portal origin
- Endpoint path: `/v1/chat/completions`
- Auth: `Authorization: Bearer <your API key>`
- Model scope: the models allowed for that client key, or all enabled routes

The copy button still copies the full endpoint URL. The display avoids a long full URL wrapping poorly on mobile by separating `Base URL` from `Endpoint path`.

User mode now keeps Settings visible because `Portal preferences` such as font size and density are browser-side controls useful to both admin and client users. Admin-only menus remain hidden from user mode: Providers, Models & Routes, Teams, and API Keys.

## Permanent gate added

Added `swarm/scripts/portal_user_journey_audit.mjs` and wrapper `swarm/scripts/portal_user_journey_audit.sh`.

The gate creates a temporary mock-backed route, team, and scoped API key, sends one real request through the router, then validates desktop and mobile user flows:

1. user login by client API key,
2. admin menus are hidden,
3. Dashboard / Usage / Settings remain available,
4. Dashboard shows the call endpoint quick start,
5. model scope includes the scoped test model,
6. Usage shows the user's own model and Tok/s signal,
7. user Usage does not show admin-only provider/team/key aggregations,
8. Settings shows Portal preferences but not admin runtime internals,
9. desktop/mobile have no page-level horizontal overflow.

## Verification

Syntax/static:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `node --check swarm/scripts/portal_user_journey_audit.mjs` — PASS
- `bash -n swarm/scripts/portal_user_journey_audit.sh` — PASS
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

- `swarm/out/portal_user_journey_audit-first-201951.log`
- `swarm/out/portal_user_journey_audit-call-panel-202032.log`
- `swarm/out/portal_user_journey_audit-user-journey-full-202204.log`
- `swarm/out/portal_full_page_audit-user-journey-full-202133.log`

Visual evidence:

- `swarm/out/playwright/20260917-202032-portal-user-journey-audit/mobile-390-user-dashboard.png`
- `swarm/out/playwright/20260917-202032-portal-user-journey-audit/desktop-1440-user-dashboard.png`
- `swarm/out/playwright/20260917-202032-portal-user-journey-audit/mobile-390-user-settings.png`
- `swarm/out/playwright/20260917-202032-portal-user-journey-audit/mobile-390-user-usage.png`
