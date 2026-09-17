# Codex audit — Role-aware Portal Settings — 2026-09-17

## What changed

Settings now renders different content for admin and user mode.

Admin mode:

- Portal preferences: font size and density.
- Runtime: router address, database status, config reload status, max body bytes, version.

User mode:

- Portal preferences: font size and density.
- Session: current API key prefix, team name, key status, model scope.

User Settings no longer shows the previous `Settings are admin-only` placeholder and no longer uses admin runtime wording.

## Why

User mode now intentionally exposes Settings for browser-side Portal preferences. Showing an admin-only placeholder inside that screen was confusing and made the User Portal feel unfinished. A read-only session summary is more useful and does not expose admin runtime internals.

## Permanent gate updated

`swarm/scripts/portal_user_journey_audit.mjs` now verifies that user Settings:

- contains Portal preferences,
- contains a Session summary,
- contains the signed-in API key prefix,
- contains model scope,
- does not contain `Settings are admin-only`, router address, database status, or config reload status,
- has no desktop/mobile horizontal overflow.

## Verification

Syntax/static:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `node --check swarm/scripts/portal_user_journey_audit.mjs` — PASS
- `git diff --check` — PASS

Browser gates:

- `swarm/scripts/portal_empty_state_audit.sh` — PASS
- `swarm/scripts/portal_logic_acceptance.sh` — PASS
- `swarm/scripts/portal_visual_audit.sh` — PASS
- `swarm/scripts/portal_polish_audit.sh` — PASS
- `swarm/scripts/portal_full_page_audit.sh` — PASS
- `swarm/scripts/portal_modal_surface_audit.sh` — PASS
- `swarm/scripts/portal_user_journey_audit.sh` — PASS

Representative logs:

- `swarm/out/portal_user_journey_audit-user-settings-202611.log`
- `swarm/out/portal_user_journey_audit-settings-role-full-202739.log`
- `swarm/out/portal_full_page_audit-settings-role-full-202708.log`
- `swarm/out/portal_modal_surface_audit-settings-role-full-202727.log`

Visual evidence:

- `swarm/out/playwright/20260917-202739-portal-user-journey-audit/mobile-390-user-settings.png`
- `swarm/out/playwright/20260917-202739-portal-user-journey-audit/desktop-1440-user-settings.png`
