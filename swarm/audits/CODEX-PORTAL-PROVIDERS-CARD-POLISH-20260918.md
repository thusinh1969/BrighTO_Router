# Codex audit — Providers desktop card polish

Date: 2026-09-18
Role: auditor / implementation cleanup

## Root issue fixed

The Providers page still used a classic wide table on desktop. It technically worked, but the core provider connection list looked dated, compressed action buttons into a table cell, and did not match the newer Models & Routes card-list treatment.

## Change made

- Kept semantic table DOM for compatibility with existing mobile stacked-card behavior.
- Added provider-specific classes: `provider-list-table` / `provider-list-wrap`.
- Desktop Providers now renders each provider connection as a compact card row.
- Provider name, base URL, and status notes wrap safely without hard truncation.
- Header actions now group `Pre-register provider` and filter together, matching Models & Routes.
- Added Playwright full-page audit checks to prevent desktop Providers from regressing to a sparse wide table.

## Verified

All checks passed against the running HTTPS Portal:

- `portal_full_page_audit.sh` — PASS
- `portal_logic_acceptance.sh` — PASS
- `portal_modal_surface_audit.sh` — PASS
- `portal_user_journey_audit.sh` — PASS
- `portal_polish_audit.sh` — PASS

Latest visual evidence:

- `swarm/out/playwright/20260918-013608-portal-full-page-audit/desktop-1440-providers.png`
- `swarm/out/playwright/20260918-013608-portal-full-page-audit/mobile-390-providers.png`
