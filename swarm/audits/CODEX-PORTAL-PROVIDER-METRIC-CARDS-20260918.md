# CODEX audit — Provider row metric cards

Date: 2026-09-18
Scope: Admin Portal `Providers` page, desktop provider connection rows.

## Finding
Desktop provider rows still felt like a flat table inside a card. Routes, Usage, and Base URL were plain text cells, which made the row look less polished than the newer Models and API Keys cards.

## Fix applied
- Styled provider Routes, Usage, and Base URL as compact mini-cards inside each provider row.
- Preserved the mobile stacked card labels and existing provider actions.
- Added a full-page Playwright gate requiring provider metric cells to have visible card treatment: border, radius, and minimum readable height.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed.
- `node --check swarm/scripts/portal_full_page_audit.mjs` passed.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshot inspected: `swarm/out/playwright/20260918-065615-portal-full-page-audit/desktop-1440-providers.png`.
