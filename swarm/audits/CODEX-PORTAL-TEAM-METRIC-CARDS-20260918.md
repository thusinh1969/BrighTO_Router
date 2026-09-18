# CODEX audit — Team row metric cards

Date: 2026-09-18
Scope: Admin Portal `Teams` page, desktop team rows.

## Finding
The desktop Teams page still looked like a flat table inside a card. Budget, API key count, and status were plain cells, unlike the more polished provider/model card treatment.

## Fix applied
- Styled Team Budget, API Keys, and Status values as compact mini-cards inside each team row.
- Kept the mobile stacked card flow intact.
- Added a full-page Playwright gate requiring desktop team metric cells to have visible card treatment: border, radius, and readable height.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed.
- `node --check swarm/scripts/portal_full_page_audit.mjs` passed.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshot inspected: `swarm/out/playwright/20260918-070135-portal-full-page-audit/desktop-1440-teams.png`.
