# Codex audit — Teams and API Keys desktop card polish

Date: 2026-09-18
Role: auditor / implementation cleanup

## Root issue fixed

Teams and API Keys still used flat desktop tables while Providers and Models had moved to compact card-list rows. This made the Admin core screens visually inconsistent and left long team/key values dependent on table behavior.

## Change made

- Kept semantic table DOM for compatibility with existing mobile stacked-card rendering and tests.
- Added `team-list-table` / `team-list-wrap` for Teams.
- Added `key-list-table` / `key-list-wrap` for API Keys.
- Desktop Teams now renders each team as a compact card row with budget, key counts, status, and actions grouped cleanly.
- Desktop API Keys now renders each key as a compact card row while preserving full key visibility and one copy action.
- Added Playwright full-page audit guards so desktop Teams and API Keys cannot regress to sparse wide tables.

## Verified

All checks passed against the running HTTPS Portal:

- `portal_full_page_audit.sh` — PASS
- `portal_logic_acceptance.sh` — PASS
- `portal_modal_surface_audit.sh` — PASS
- `portal_user_journey_audit.sh` — PASS
- `portal_polish_audit.sh` — PASS

Latest visual evidence:

- `swarm/out/playwright/20260918-014209-portal-full-page-audit/desktop-1440-teams.png`
- `swarm/out/playwright/20260918-014209-portal-full-page-audit/desktop-1440-keys.png`
- `swarm/out/playwright/20260918-014209-portal-full-page-audit/mobile-390-keys.png`
