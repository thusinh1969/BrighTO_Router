# CODEX audit — Portal long model/provider name polish fix — 2026-09-18

## Root cause

Production model names can be long. The Portal rendered those full strings directly in table cells, chart legends, usage breakdown cards, and request rows. Long names either wrapped into ugly multi-line blocks or were clipped by CSS. This made Models and Usage look unprofessional with realistic provider model names.

## Fix

Changed `static/index.html` only:

- Added `compactName()` and `nameNode()` helpers for middle-short display names.
- Full names remain available in `title` and in screen-reader text; model route rows still keep the explicit copy button for exact values.
- Models table public-model cell now shows one clean compact name beside the copy button instead of wrapping the row.
- Chart legends now use chip-style compact labels.
- Dashboard focus rows, Usage breakdown rows, and request logs use compact display labels for long names.
- Usage breakdown titles are shortened more aggressively because metric chips leave less horizontal room.

## Verification

Live HTTPS container, base URL `https://127.0.0.1:18443`:

- `./swarm/scripts/portal_full_page_audit.sh` — PASS
- `./swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `./swarm/scripts/portal_logic_acceptance.sh` — PASS
- `./swarm/scripts/portal_user_journey_audit.sh` — PASS
- `./swarm/scripts/portal_polish_audit.sh` — PASS

Representative screenshot checked manually:

- `swarm/out/playwright/20260918-023025-portal-full-page-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260918-022824-portal-full-page-audit/desktop-1440-usage.png`

## Verdict

This is a real polish fix, not a logic rewrite. It does not touch provider/model route APIs, database schema, routing behavior, keys, budgets, or Docker compose. It makes the Portal tolerate realistic long model names without visually breaking admin screens.

## Follow-up polish in same area

A second pass removed hidden duplicate full names from rendered DOM text and changed compact labels to use `title` plus `aria-label`. It also compacted long provider names, team names, API-key owners, API-key teams, and model scope labels in admin tables.

The polish audit row locator was updated to find rows by visible text, `title`, or `aria-label`, because compact labels intentionally do not render the full raw value as visible text.

Additional live verification after this follow-up:

- `./swarm/scripts/portal_full_page_audit.sh` — PASS
- `./swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `./swarm/scripts/portal_logic_acceptance.sh` — PASS
- `./swarm/scripts/portal_user_journey_audit.sh` — PASS
- `./swarm/scripts/portal_polish_audit.sh` — PASS

Representative screenshots checked manually:

- `swarm/out/playwright/20260918-023641-portal-full-page-audit/desktop-1440-providers.png`
- `swarm/out/playwright/20260918-023641-portal-full-page-audit/desktop-1440-keys.png`
