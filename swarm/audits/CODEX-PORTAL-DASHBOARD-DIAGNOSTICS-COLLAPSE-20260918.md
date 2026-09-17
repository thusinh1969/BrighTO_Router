# CODEX audit — Dashboard diagnostics collapse on mobile

Date: 2026-09-18
Commit target: pending
Area: Portal Admin Dashboard, mobile UX

## Root cause fixed

Admin Dashboard mobile placed technical diagnostics before Recent requests. This pushed the latest call log, including tokens/sec, too far down the page. Provider health, router overhead, and latency buckets are useful, but they are diagnostic details, not the first thing an admin needs after the health summary and usage chart.

## Change made

- Moved Recent requests above technical diagnostics.
- Grouped Provider health, Performance diagnostics, and Latency by prompt size into a `Technical diagnostics` details block.
- Desktop keeps diagnostics open by default.
- Mobile keeps diagnostics collapsed by default.
- Full-page Playwright audit now checks the mobile ordering and collapsed default, plus desktop open default.

## Verification

Passed locally after the change:

- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-dashboard-diagnostics-collapse-004114.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-dashboard-diagnostics-final-004144.log`
- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-dashboard-diagnostics-final-004144.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-dashboard-diagnostics-final-004144.log`

## Current evidence

Mobile Dashboard now shows: health summary, KPI cards, chart, top consumers/models, Recent requests, then collapsed Technical diagnostics.
