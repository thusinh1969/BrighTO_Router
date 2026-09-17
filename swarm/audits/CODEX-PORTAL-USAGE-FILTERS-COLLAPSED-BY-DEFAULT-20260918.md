# CODEX audit — Usage filters collapsed by default

Date: 2026-09-18
Commit target: pending
Area: Portal Usage page

## Root cause fixed

The desktop Usage page opened the full filter form by default. With no active filter this pushed the useful content — totals, chart, and request logs with tokens/sec — lower than necessary. Mobile already had the right direction, but desktop still felt like a configuration form instead of an operations view.

## Change made

- Usage filters now default collapsed whenever no filter is active, on both desktop and mobile.
- Active filters still open the form automatically.
- Quick range buttons (`7d`, `30d`, `90d`) and `Reset filters` remain visible while collapsed.
- Full-page Playwright audit now checks collapsed default and visible quick range buttons on both desktop and mobile.

## Verification

Passed locally after the change:

- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-usage-filter-collapse-all-010936.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-usage-filter-collapse-final-011010.log`
- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-usage-filter-collapse-final-011010.log`
- `portal_logic_acceptance PASS swarm/out/portal_logic_acceptance-usage-filter-collapse-final-011010.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-usage-filter-collapse-final-011010.log`

## Current evidence

Full-page audit measured zero visible filter form fields by default on both desktop and mobile, while `Show filters`, `7d`, `30d`, and `90d` remain visible.
