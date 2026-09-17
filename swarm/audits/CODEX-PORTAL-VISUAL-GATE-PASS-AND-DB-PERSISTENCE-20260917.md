# CODEX AUDIT — Portal visual gate pass + Docker restart does not wipe PostgreSQL

Date: 2026-09-17 17:52 +07
Live target: `https://127.0.0.1:18443`
Docker image rebuilt/recreated: `thusinh1969/brighto_airouter:v1`

## Verdict

Known Models & Routes visual blockers are now closed in the current gate.

This is not a claim that the Portal is fully WOW/OpenRouter-level. It means the specific verified regressions are fixed:

- long public model does not collide with Status,
- Provider does not collide with Provider Model,
- Provider Model is controlled,
- action column fits buttons,
- mobile sidebar closes after navigation,
- page-level horizontal overflow is absent.

## Product patch applied

`static/index.html` Provider cell in `renderModels()` now uses the same `.truncate` pattern as Provider Model.

Before:

```js
tr.appendChild(el("td",null,providerNames));
```

After:

```js
var pv=el("td");
var pvw=el("span","truncate",providerName||"—");
pvw.title=providerName||"";
pv.appendChild(pvw);
tr.appendChild(pv);
```

## Verification commands run

```bash
cargo check --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
```

Results:

- `cargo check --workspace`: PASS
- Docker build/recreate: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_visual_audit.sh`: PASS

Latest artifacts:

- `swarm/out/playwright/20260917-174729-portal-logic-acceptance/summary.json`
- `swarm/out/playwright/20260917-174743-portal-visual-audit/summary.json`
- `swarm/out/playwright/20260917-174743-portal-visual-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-174743-portal-visual-audit/mobile-390-models.png`

## PostgreSQL persistence check

User asked whether every rebuild/restart wipes tested model definitions.

Expected behavior: no. Rebuilding the image and running `docker compose up -d --force-recreate router` recreates only the router container. PostgreSQL data is stored in the named volume `brighto-airouter_pg-data` and should persist.

Verified with a temporary route:

1. Created route `persist-check-manual-restart` through Admin API.
2. Ran `docker compose up -d --force-recreate router`.
3. Waited for HTTPS endpoint to answer.
4. Queried `/admin/routes`.
5. Route still existed: `route_exists_after_restart 1`.
6. Cleaned up the temporary route/backend explicitly.

Conclusion: normal router rebuild/recreate does not wipe PostgreSQL.

## What can delete or change data

These are the operations/scripts that can intentionally remove records:

- `docker compose down -v` removes named volumes and will wipe PostgreSQL data.
- Explicit DB reset scripts/audits from earlier dev cleanup can wipe definitions.
- `swarm/scripts/portal_logic_acceptance.sh` deletes test records only matching dev prefixes/patterns such as `pw-%`, `crud-%`, `verify-%`, `mock-model`, and backend URL `http://127.0.0.1:9000/v1`.
- `swarm/scripts/portal_visual_audit.mjs` creates and removes `pw-visual-*` routes and mock backend `http://127.0.0.1:9000/v1` when safe.

`./start.sh start` and `./start.sh restart` also run `scripts/seed_defaults.sql`. That SQL is safe to rerun in the sense that it does not `DROP`, `TRUNCATE`, or `DELETE`. It inserts the default team/providers when missing. It also updates `base_url`, `api_key_ref`, and `format` for known provider backend names such as `openai`, `anthropic`, `deepseek`, `custom-openai`; it keeps `enabled` as-is. Therefore it should not delete model routes, but it can overwrite direct edits to a default backend row with the same formal name.

If a real user-created model disappears after plain router recreate and it does not match those test prefixes/mock URLs, that is a bug and needs a separate reproduction.

## Next quality bar

Known gates are green, but Portal still needs a broader design review before claiming professional/WOW:

- review all pages, not only Models & Routes,
- mobile should preferably use cards for dense rows instead of relying only on horizontal table scroll,
- Dashboard should emphasize admin-critical metrics: tokens, cost, tokens/sec, errors, provider health, recent calls,
- remove any visual clutter that does not help an admin operate the router.
