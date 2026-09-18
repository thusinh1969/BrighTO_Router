# CODEX audit — User Dashboard metric dedup polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_user_journey_audit.mjs`

## Root cause

User Dashboard showed the same 30-day metrics twice:

1. in the role-specific `Last 30 days` hero card;
2. again as generic summary cards below the Call endpoint panel.

This made the mobile User Dashboard longer than needed and pushed the chart/recent requests further down without adding information.

## Fix applied

- Admin Dashboard keeps its operational summary cards unchanged.
- User Dashboard no longer renders the generic Requests/Total tokens/Error rate card strip because that data already exists in the `Last 30 days` card.
- User Dashboard still keeps the call endpoint, cURL preview, chart, and recent requests.

## Regression gate

`portal_user_journey_audit.mjs` now fails if User Dashboard reintroduces generic summary cards or duplicated `Requests (30d)`, `Total tokens`, or duplicated error-rate summary below the call panel.

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

Screenshot inspected manually:

- `swarm/out/playwright/20260918-073948-portal-user-journey-audit/mobile-390-user-dashboard.png`
