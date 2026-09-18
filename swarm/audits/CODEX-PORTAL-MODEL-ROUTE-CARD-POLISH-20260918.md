# CODEX audit — Models & Routes card polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_full_page_audit.mjs`

## Root cause

The Models & Routes desktop route row rendered as a sparse wide grid. Route details were readable but felt like table fragments: status/provider/provider-model/settings were visually flat, and an attempted wider grid pushed action buttons outside the visible route card. The old Playwright gate only checked that a grid existed, not that actions stayed visible or route details looked like card cells.

## Fix applied

- Kept the existing table DOM so route logic and audit selectors remain stable.
- Styled status, provider, provider model, and route settings as compact card cells.
- Rebalanced the large-screen grid so public model names stay readable while action buttons remain inside the route card.
- Changed route settings to a one-row metric grid on desktop and a compact two-column grid below desktop.
- Kept mobile readable with no horizontal overflow and full public model names.

## Regression gate

`portal_full_page_audit.mjs` now captures route action button geometry and route metric cell styling. It fails Models desktop if:

- route actions are missing, too small, or outside the card boundary;
- route detail cells are flat table cells instead of bordered compact cards;
- public model column becomes too narrow for real model names.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_full_page_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Results:

- `PORTAL_STATIC_GATE PASS`
- full page Playwright audit: `PASS`, 0 failures, 0 console errors
- user journey Playwright audit: `PASS`, 0 failures, 0 console errors

Screenshots inspected manually:

- `swarm/out/playwright/20260918-072103-portal-full-page-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260918-072103-portal-full-page-audit/mobile-390-models.png`
