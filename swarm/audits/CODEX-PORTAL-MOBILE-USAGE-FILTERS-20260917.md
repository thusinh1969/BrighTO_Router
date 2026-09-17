# CODEX audit — Mobile Usage filters

Date: 2026-09-17

## Verdict

Fixed a mobile Usage density problem.

On mobile, the Usage page rendered the full admin filter form before any usage data. The filters are useful, but the default view should prioritize the data: request count, tokens, chart, rollups, and logs.

## Change

- `static/index.html`
  - Added Usage filter open/closed state.
  - On mobile, when filters are at the default state, the filter panel starts collapsed and shows a `Show filters` action plus a compact summary such as `Showing 30 days`.
  - If a filter is active, the panel opens so the admin can see and edit the active criteria.
  - `Show filters` expands the full form; Apply/quick range/reset keep the next state logical.
- `swarm/scripts/portal_full_page_audit.mjs`
  - Added a mobile Usage gate requiring the default filter panel to stay collapsed before data.
  - Added an interaction gate requiring `Show filters` to expand the full filter form.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-mobile-usage-filter-toggle-234823.log`
- `bash swarm/scripts/portal_polish_audit.sh` — PASS
  - Log: `swarm/out/portal_polish_audit-mobile-usage-filter-toggle-234849.log`

## Screenshot evidence

- `swarm/out/playwright/20260917-234736-portal-full-page-audit/mobile-390-usage.png`
