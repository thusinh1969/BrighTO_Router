# Codex audit — Portal Teams summary polish — 2026-09-17

## What changed

Teams now matches the operational surface of Providers, Models, and API Keys. The page shows four summary cards before the table:

- Teams: total owners for apps/projects.
- Active teams: teams that can issue usable keys.
- Active keys: enabled client keys across all teams.
- Budget caps: teams with token or spend limits.

Summary card subtitles now wrap instead of truncating with ellipsis, so mobile does not show clipped text such as `sp...`.

## Why

Teams was the last main admin CRUD tab without a top-level operational summary. The table was usable, but the admin had to scan rows for basic state. The summary cards make the page consistent with the other production control screens and improve mobile readability.

## Permanent gates updated

- `swarm/scripts/portal_full_page_audit.mjs` now requires operational summary cards on Teams in both desktop and mobile audits.
- `swarm/scripts/portal_polish_audit.mjs` now checks Teams summary card presence.

## Verification

Syntax/static:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS

Browser gates:

- `swarm/scripts/portal_empty_state_audit.sh` — PASS
- `swarm/scripts/portal_logic_acceptance.sh` — PASS
- `swarm/scripts/portal_visual_audit.sh` — PASS
- `swarm/scripts/portal_polish_audit.sh` — PASS
- `swarm/scripts/portal_full_page_audit.sh` — PASS
- `swarm/scripts/portal_modal_surface_audit.sh` — PASS

Representative logs:

- `swarm/out/portal_polish_audit-team-summary-201147.log`
- `swarm/out/portal_full_page_audit-team-summary-201155.log`
- `swarm/out/portal_polish_audit-teams-wrap-201317.log`
- `swarm/out/portal_full_page_audit-teams-wrap-201325.log`
- `swarm/out/portal_modal_surface_audit-teams-wrap-201344.log`

Visual evidence:

- `swarm/out/playwright/20260917-201325-portal-full-page-audit/mobile-390-teams.png`
- `swarm/out/playwright/20260917-201325-portal-full-page-audit/desktop-1440-teams.png`
