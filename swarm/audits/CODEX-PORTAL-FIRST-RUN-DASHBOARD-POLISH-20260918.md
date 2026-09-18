# Codex audit — first-run Dashboard must guide setup, not show zero KPI cards

Date: 2026-09-18

## Finding

The admin Dashboard empty state still rendered the generic KPI grid before the Launch checklist. On a fresh install this produced low-value cards such as enabled routes `0`, active providers `0`, requests `0`, and total tokens `0` before the admin had created a first tested model route.

## Fix applied

- `static/index.html`
  - `renderDashboard()` now returns immediately after the onboarding hero and Launch checklist when there are zero model routes.
  - Non-empty admin dashboards still render the KPI grid and analytics panels.
  - User dashboard behavior is unchanged.

- `swarm/scripts/portal_empty_state_audit.mjs`
  - Added evidence for first-run summary card count and summary labels.
  - Added regression gates that fail if the empty Dashboard shows generic zero KPI cards or their labels before setup.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- `node --check swarm/scripts/portal_empty_state_audit.mjs` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh` — PASS
  - `summaryCards: 0`
  - `summaryLabels: []`
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` — PASS
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh` — PASS
