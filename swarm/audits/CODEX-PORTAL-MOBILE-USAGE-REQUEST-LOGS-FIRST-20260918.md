# CODEX audit — Mobile Usage puts request logs before breakdowns

Date: 2026-09-18
Commit target: pending
Area: Portal Usage page, mobile UX

## Root cause fixed

The Usage page showed request logs after By model, By team, and By API key breakdown tables. On mobile this pushed the latest call record and tokens/sec below several tall stacked tables. That conflicted with the admin priority: see recent call health and token speed first.

## Change made

- Desktop Usage remains unchanged: chart, breakdown tables, then request logs.
- Mobile Usage now shows chart, Request logs, then a collapsed `Usage breakdowns` drawer.
- The drawer contains By model, By team, and By API key totals for the selected range.
- Full-page Playwright audit now checks that mobile Request logs appear before Usage breakdowns and that the breakdown drawer defaults closed.

## Verification

Passed locally after the change:

- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-mobile-usage-breakdowns-005042.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-mobile-usage-breakdowns-final-005113.log`
- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-mobile-usage-breakdowns-final-005113.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-mobile-usage-breakdowns-final-005113.log`

## Current evidence

Mobile Usage now shows Request logs above Usage breakdowns, so token/sec is visible earlier in the flow.
