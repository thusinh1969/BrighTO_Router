# CODEX AUDIT — CATALOG ONE-FLOW PORTAL ACCEPTED

Date: 2026-09-17  
Role: Codex auditor. Product code was implemented by DeepSeek.

## Verdict

**PRODUCT FLOW ACCEPTED for the current scope.**

The latest HEAD compiles, unit/integration tests pass, the live HTTPS Portal runs, and the canonical Playwright flow passes on a clean DB.

Accepted commit state:

- `62d721d` — `Update canonical gate for catalog one-flow + lifecycle; gate PASSES 17/17 in Docker playwright (chromium-1243)`
- Worktree: clean at audit time.

## Rust verification

Command:

```bash
cargo check --workspace
cargo test --workspace
```

Result:

- `cargo check --workspace`: PASS
- `cargo test --workspace`: PASS
- Test count: 60 lib tests + 4 integration tests = 64 passing tests.

Note: `cargo test` must run outside the current Codex sandbox because SQLx test databases require local network access. Inside sandbox it fails with `Operation not permitted`, not with assertion failures.

## Live service verification

Live URL tested:

```text
https://127.0.0.1:18443
```

Docker status at audit time:

- `brighto-airouter-router-1` running from `thusinh1969/brighto_airouter:v1`
- `brighto-airouter-postgres-1` healthy
- router logs show HTTPS listening on `0.0.0.0:18443`

## DB state after reset and test cleanup

After the Playwright gate cleanup:

- `/admin/backends`: 0
- `/admin/routes`: 0
- `/admin/teams`: 1 (`Default`, unlimited budget, enabled)
- `/admin/keys`: 1 demo key (`lc-01234...`, revealable, owner `demo`)

This matches the catalog-only design: a fresh Admin starts from Models → Add model; provider connections are created only when saving a route.

## Playwright verification

Gate run:

- Script: `swarm/scripts/portal_logic_acceptance.mjs`
- Browser: Docker `mcr.microsoft.com/playwright:v1.63.0-noble`, Chromium headless shell revision 1243
- Artifact: `swarm/out/playwright/codex-canonical-20260917-170950/summary.json`
- Result: PASS
- Passes: 17
- Failures: 0

Verified by browser clicks/API checks:

1. Admin login over HTTPS works.
2. Compact density and number formatting work: `1K`, `50K`, `1M`.
3. Provider catalog comes from config/env and uses only 2 dialects: `openai`, `anthropic`.
4. Catalog includes Custom LLM.
5. Gemini is marked disabled/coming-soon.
6. `Save enabled` is disabled before test connection.
7. `Save draft` is available without test connection.
8. Models → Add model → Custom LLM → Load models opens chooser modal.
9. Picker selects exactly one upstream model.
10. Test connection passes and enables `Save enabled`.
11. Save creates the route and auto-creates the connection.
12. Reusing the same base URL deduplicates connection rows.
13. Client `/v1/chat/completions` call through router returns 200.
14. After usage, model route has `can_delete=false`.
15. Deleting a model route with usage returns 409.
16. In-use provider connection Delete is disabled in UI.
17. Teams, client API key creation/reveal, and User login/menu hiding pass.

## Important test-data note

An earlier canonical run failed because stale `mock-model` data from a previous selfcheck made Playwright click a table row under the modal instead of the picker row. After direct cleanup, the same canonical gate passed 17/17. This was a test-data/selector problem, not a product failure.

Required hardening for the gate script:

- Cleanup before **and** after test must remove `mock-model`, `pw-*`, `crud-*`, `verify-*` routes/backends/usage/teams/keys.
- Picker row selectors must be scoped to the picker modal/list, not global `text=mock-model`.

## Remaining non-product cleanup

The product gate is accepted, but `swarm/scripts/portal_logic_acceptance.sh` still installs/runs local Playwright and may pick mismatched local Chromium revision 1234. That wrapper can reintroduce false UI failures.

Required test harness fix:

- Make `portal_logic_acceptance.sh` run the Docker Playwright image by default:
  - image: `mcr.microsoft.com/playwright:v1.63.0-noble`
  - network: `host`
  - browser: `/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell`
  - install `playwright@1.63.0` or `playwright-core@1.63.0` inside the artifact dir with browser download disabled.
- Keep local fallback only if the local browser revision matches the installed Playwright package.

This is not blocking product acceptance, but it is blocking reliable future regression testing.
