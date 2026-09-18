# CODEX audit — Portal chart legend polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_full_page_audit.mjs`

## Root cause

The Usage/Dashboard chart legend rendered after the SVG as wrapping pills. With long public model names it looked like a detached label block under the chart, and with multiple models it could grow vertically instead of staying compact.

## Fix applied

- The legend now renders before the SVG, directly under the chart title area.
- Legend items are compact horizontal chips with controlled width.
- The legend container is a horizontal scroll row with `flex-wrap: nowrap` and `overflow-x: auto`.
- Long model names still wrap inside each chip and keep the full text/title; no JavaScript truncation was added.
- Mobile keeps the same horizontal chip behavior so long labels do not create page-width overflow.

## Regression gate

`portal_full_page_audit.mjs` now captures `legendContainerStats` and fails Dashboard/Usage if a visible legend is not a flex horizontal scroll row, wraps on desktop, clips labels, or grows beyond controlled height.

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

- `swarm/out/playwright/20260918-071538-portal-full-page-audit/desktop-1440-usage.png`
- `swarm/out/playwright/20260918-071538-portal-full-page-audit/mobile-390-dashboard.png`
