# CODEX audit — Dashboard speed KPI

Date: 2026-09-17

## Verdict

Fixed a dashboard priority issue.

Tokens/sec is one of the most important signals for BrighTO-Router, but before this change it appeared mainly in request logs. The top dashboard hero showed Traffic, Tokens, Cost, and Errors, while speed required scrolling. That made the dashboard less useful during admin checks and local/provider smoke tests.

## Change

- `static/index.html`
  - Dashboard hero now shows four primary signals: `Traffic`, `Speed`, `Cost`, `Errors`.
  - `Speed` uses the latest request's token-rate display, including the `*` short-sample marker added in the previous audit.
  - Total tokens still appear in the KPI cards and chart immediately below the hero.
  - Hero KPI subtext now wraps cleanly instead of truncating.
  - Replaced the jargon-like hero kicker `Production control room` with `Router overview`.
- `swarm/scripts/portal_polish_audit.mjs`
  - Added a gate that Dashboard must surface Speed/Tokens/sec near the top.
  - Added a gate that hero KPI subtext must not clip horizontally.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_polish_audit.sh` — PASS
  - Log: `swarm/out/portal_polish_audit-dashboard-speed-kpi-final-233619.log`
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-dashboard-speed-kpi-final-233619.log`

## Screenshot evidence

- `swarm/out/playwright/20260917-233658-portal-full-page-audit/desktop-1440-dashboard.png`
