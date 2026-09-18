# CODEX audit — API Key row metric cards

Date: 2026-09-18
Scope: Admin Portal `API Keys` page, desktop key rows.

## Finding
The desktop API Keys page still had flat table-like value cells for Owner/Team, Scope, and Limits. That made it visually inconsistent with the polished Provider and Team rows, and reduced scannability for admin review.

## Fix applied
- Styled API key Owner/Team, Scope, and Limits as compact mini-cards inside each key row.
- Preserved the full visible client key and explicit `Copy key` action.
- Kept the mobile stacked card flow intact.
- Added a full-page Playwright gate requiring desktop API key metric cells to have visible card treatment: border, radius, and readable height.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed.
- `node --check swarm/scripts/portal_full_page_audit.mjs` passed.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshot inspected: `swarm/out/playwright/20260918-070701-portal-full-page-audit/desktop-1440-keys.png`.
