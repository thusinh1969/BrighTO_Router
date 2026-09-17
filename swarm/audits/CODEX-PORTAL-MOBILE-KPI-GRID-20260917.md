# CODEX audit — Mobile KPI grid

Date: 2026-09-17

## Verdict

Fixed a mobile dashboard density issue.

The mobile dashboard stacked every KPI card in a single column. That made the page unnecessarily long even for compact scalar values such as routes, providers, teams, requests, tokens, and error rate.

## Change

- `static/index.html`
  - Mobile `.grid` cards now render as two columns.
  - Mobile card value font and padding were reduced slightly so the cards remain readable on 390px screens.
  - Tables and request logs remain stacked as one-column cards; only KPI summary cards changed.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added a mobile dashboard gate requiring KPI cards to use a compact two-column layout when enough width exists.
  - Added a width guard so cards do not become too narrow to read.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-mobile-kpi-grid-234158.log`

## Screenshot evidence

- `swarm/out/playwright/20260917-234158-portal-full-page-audit/mobile-390-dashboard.png`
