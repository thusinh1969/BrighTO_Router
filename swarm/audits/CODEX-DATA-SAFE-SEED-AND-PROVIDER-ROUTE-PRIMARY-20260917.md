# CODEX audit — data-safe seed + Provider Route priority

Date: 2026-09-17

## Verdict

Fixed two concrete issues found during audit.

1. `./start.sh start`, `./start.sh restart`, and `./start.sh seed` used `scripts/seed_defaults.sql`. The seed was described as safe, but it updated `base_url`, `api_key_ref`, and `format` for known providers on every run. That could revert user-edited provider settings after restart and look like data loss.
2. In the Providers table, `Edit` was visually primary and `Route` was secondary, although the normal admin flow is to create a route from a provider preset/connection.

## Changes

- `scripts/seed_defaults.sql`
  - Seed now inserts missing default provider templates only.
  - Existing providers are left untouched, including URL, key reference, protocol format, and enabled/disabled state.
- `README.md`, `INSTALL.md`
  - Documented that rebuild/restart keeps PostgreSQL data in Docker named volume `brighto-airouter_pg-data`.
  - Documented the destructive operations: `docker compose down -v`, deleting the Docker volume, or manual reset/truncate SQL.
  - Corrected README default records to say provider templates are seeded disabled presets.
- `static/index.html`
  - Providers row now makes `Route` the primary action and `Edit` secondary.
- `swarm/scripts/portal_polish_audit.mjs`
  - Added a Playwright assertion that Provider row `Route` is primary and `Edit` is not primary.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `git diff --check` — PASS
- Temporary PostgreSQL seed-preservation test — PASS
  - Verified an existing user-edited `openai` provider keeps its URL/key/enabled state after seed.
  - Verified missing provider templates are still inserted.
- `bash swarm/scripts/portal_polish_audit.sh` — PASS
  - Log: `swarm/out/portal_polish_audit-data-safe-route-primary-232042.log`
- `bash swarm/scripts/portal_logic_acceptance.sh` — PASS
  - Log: `swarm/out/portal_logic_acceptance-data-safe-route-primary-232042.log`
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-data-safe-route-primary-232042.log`

## Operational answer

Docker image rebuild and router restart do not wipe tested model definitions. Local PostgreSQL uses the Docker named volume `brighto-airouter_pg-data`. Data is wiped only by explicit destructive actions such as `docker compose down -v`, deleting that volume, or running manual reset/truncate SQL.
