# CODEX audit — Responsive chart labels

Date: 2026-09-18
Commit target: pending
Area: Portal Dashboard and Usage charts

## Root cause fixed

The chart rendered with a desktop-width SVG viewBox on mobile. Playwright passed because there was no overflow, but chart labels were scaled down to roughly 5px high on a 390px viewport. This made sparse data charts look empty and hard to read.

## Change made

- Chart axis labels use stronger contrast.
- Sparse charts now show a value label above each bar, for example `89` tokens.
- Mobile charts use a smaller SVG viewBox so labels stay readable instead of being scaled down from desktop dimensions.
- Full-page Playwright audit now checks that dashboard/usage charts have value labels and that mobile value labels are not too small.

## Verification

Passed locally after the change:

- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-responsive-chart-labels-003409.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-responsive-chart-labels-final-003438.log`
- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-responsive-chart-labels-final-003438.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-responsive-chart-labels-final-003438.log`

## Current evidence

Before: mobile chart value label rendered around 5px high.
After: mobile chart value label renders around 13px high on 390px viewport.
