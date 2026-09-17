# CODEX audit — Model route desktop layout polish

Date: 2026-09-18
Scope: Portal `Models & Routes` page, desktop and mobile visual behavior.

## Finding
The full-page audit passed, but the desktop Models route row still looked weak by visual review: public model names were constrained by a six-column table-style card. Long real provider names wrapped too early and looked compressed beside status/provider/action columns.

## Fix applied
- Changed desktop `.route-list-table` rows from a rigid six-column layout to a card grid.
- For 821-1199px, route details stack with actions pinned in a right rail.
- For >=1200px, public model name spans two columns, provider/provider-model sit below it, route settings and actions stay stable on the right.
- Added a Playwright gate in `portal_full_page_audit.mjs`: at desktop width >=1200, the public model cell must be at least 380px wide.

## Evidence
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` passed with 0 failures and 0 console errors.
- `python3 swarm/scripts/portal_static_gate.py` passed.
- Visual screenshot inspected: `swarm/out/playwright/20260918-062148-portal-full-page-audit/desktop-1440-models.png`.
