# Codex audit — Models & Routes desktop card polish

Date: 2026-09-18
Role: auditor / implementation cleanup

## Root issue fixed

The Models & Routes page used a wide table layout on desktop. With long public model names and provider names, the row looked sparse, actions were pushed far right, and provider/model text could fall back to table clipping rules. This made the core Admin flow feel less professional than the rest of the Portal.

## Change made

- Kept the existing semantic table DOM so mobile table-card behavior and current API/UI tests remain stable.
- Added a route-specific class: `route-list-table` / `route-list-wrap`.
- Desktop now renders model routes as compact card rows with hidden table header labels shown inside each card cell.
- Public model, provider, and provider model text wrap safely; no hard ellipsis for route-card fields.
- Grouped `Add model` and the enabled/all/disabled filter as one header action group.
- Added Playwright full-page audit checks so desktop Models must remain a compact route-card layout and cannot silently regress to a sparse table.

## Verified

All checks passed against the running HTTPS Portal:

- `portal_full_page_audit.sh` — PASS
- `portal_logic_acceptance.sh` — PASS
- `portal_modal_surface_audit.sh` — PASS
- `portal_user_journey_audit.sh` — PASS
- `portal_polish_audit.sh` — PASS

Latest visual evidence:

- `swarm/out/playwright/20260918-012903-portal-full-page-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260918-012903-portal-full-page-audit/mobile-390-models.png`

## Follow-up bar

Future front-end work should keep this standard: every core Admin screen must tolerate long provider/model/key/team names, must not depend on horizontal scroll on mobile, and must have an automated visual/logic audit that catches the exact regression.
