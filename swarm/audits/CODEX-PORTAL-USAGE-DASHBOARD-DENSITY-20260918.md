# Codex audit — Usage and Dashboard density polish

Date: 2026-09-18
Role: auditor / implementation cleanup

## Root issue fixed

Usage desktop rendered three separate breakdown tables (`By model`, `By team`, `By API key`). This consumed vertical space and made long model names wrap poorly in narrow tables. Dashboard also opened Technical diagnostics by default on desktop, which made the primary operational view too noisy for normal Admin use.

## Change made

- Replaced desktop Usage breakdown tables with one `Usage breakdowns` panel containing three readable list cards.
- Long model/team/key names now wrap naturally inside list items instead of being squeezed through narrow table columns.
- Input/output totals are shown as compact metric chips.
- Kept mobile Usage behavior unchanged: Request logs stay before a collapsed `Usage breakdowns` drawer.
- Dashboard Technical diagnostics now defaults collapsed on desktop and mobile; Admin can open it when debugging.
- Added Playwright full-page audit guards for desktop Usage breakdown cards and collapsed desktop Dashboard diagnostics.

## Verified

All checks passed against the running HTTPS Portal:

- `portal_full_page_audit.sh` — PASS
- `portal_logic_acceptance.sh` — PASS
- `portal_modal_surface_audit.sh` — PASS
- `portal_user_journey_audit.sh` — PASS
- `portal_polish_audit.sh` — PASS

Latest visual evidence:

- `swarm/out/playwright/20260918-015113-portal-full-page-audit/desktop-1440-dashboard.png`
- `swarm/out/playwright/20260918-015113-portal-full-page-audit/desktop-1440-usage.png`
- `swarm/out/playwright/20260918-015113-portal-full-page-audit/mobile-390-usage.png`
