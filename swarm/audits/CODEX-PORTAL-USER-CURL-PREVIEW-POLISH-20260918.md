# CODEX audit — User cURL preview height polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_user_journey_audit.mjs`

## Root cause

User Dashboard showed the complete ready cURL command expanded at full height. This was useful but too heavy, especially on mobile, and pushed the chart and recent requests far down the page.

## Fix applied

- The cURL command remains visible in the Call endpoint panel.
- Copy cURL still copies the full command.
- The preview block is height-limited with internal scroll:
  - desktop: 280px max;
  - mobile: 220px max.
- Command formatting remains `pre-wrap`, so it stays readable on narrow screens.

## Regression gate

`portal_user_journey_audit.mjs` now records cURL preview height, scroll height, client height, and vertical overflow. It fails if mobile preview is not height-limited with internal scroll or if desktop preview dominates the dashboard.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_user_journey_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
```

Results:

- `PORTAL_STATIC_GATE PASS`
- user journey Playwright audit: `PASS`, 0 failures, 0 console errors
- full page Playwright audit: `PASS`, 0 failures, 0 console errors

Measured preview heights:

- desktop cURL preview: 280px, scrollable
- mobile cURL preview: 220px, scrollable

Screenshot inspected manually:

- `swarm/out/playwright/20260918-074444-portal-user-journey-audit/mobile-390-user-dashboard.png`
