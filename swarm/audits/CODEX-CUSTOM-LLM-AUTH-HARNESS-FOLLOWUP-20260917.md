# CODEX AUDIT — CUSTOM LLM AUTH RULE + PLAYWRIGHT HARNESS FOLLOW-UP

Date: 2026-09-17  
Role: Codex auditor. Product code remains DeepSeek-owned.

## Current status

The real Portal one-flow product gate passed with Docker Playwright:

- Artifact: `swarm/out/playwright/codex-canonical-20260917-170950/summary.json`
- Result: PASS 17/17
- Rust: `cargo check --workspace` PASS, `cargo test --workspace` PASS (64 tests)

But two follow-up issues must be fixed before calling the regression setup SOTA/stable.

## Issue 1 — Custom LLM local auth rule is wrong when CUSTOM_LLM_API_KEY is set

Current evidence:

- `.env` has `CUSTOM_LLM_API_KEY` set.
- `PROVIDER_CATALOG` includes `custom-llm|Custom LLM|http://127.0.0.1:8088/v1|openai|CUSTOM_LLM_API_KEY|1`.
- `static/index.html` currently derives auth in `dialAuth()` like this:

```js
if(local && !pk.value.trim() && !(e.key_env && e.key_set)) {
  return { dialect:"openai", auth:"none", protocol:"local_openai_chat" };
}
return { dialect:"openai", auth:"bearer", protocol:"openai_chat" };
```

This means a local Custom LLM URL becomes `auth_mode=bearer` when `.env` has `CUSTOM_LLM_API_KEY`, even if Admin did not paste a key in the wizard.

That is wrong for the user's required local llama.cpp case:

- URL: `http://0.0.0.0:8088/v1` or reachable host-local equivalent
- key: empty
- expected route: `auth_mode=none`, `protocol=local_openai_chat`

The recent wrapper run proved the bad behavior indirectly: the created local mock route had:

```json
{"auth_mode":"bearer","protocol":"openai_chat"}
```

It still returned HTTP 200 only because the mock server ignores Authorization headers. A real local llama.cpp endpoint may not.

### Required fix

Make Custom LLM behavior explicit and deterministic:

1. For `custom-llm` and any local/private Base URL, default to no auth when the wizard API-key field is blank.
2. Use Bearer only if Admin explicitly pastes a key in the wizard for that specific model route.
3. Do not let `CUSTOM_LLM_API_KEY` silently attach to every local Custom LLM route.
4. If env-based Custom LLM key is kept, expose a clear checkbox like “Use CUSTOM_LLM_API_KEY from .env”; default unchecked for local/private URLs.

Acceptance check:

- With `.env CUSTOM_LLM_API_KEY` set and wizard key blank:
  - Custom LLM + `http://127.0.0.1:9000/v1` must save route as `auth_mode=none`, `protocol=local_openai_chat`.
- With wizard key pasted explicitly:
  - same URL may save as `auth_mode=bearer` if Admin asked for that.

## Issue 2 — `portal_logic_acceptance.sh` still uses the flaky local Playwright path

I ran the wrapper directly:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
```

Result:

- FAIL after 15 passes.
- Artifact: `swarm/out/playwright/20260917-171627-portal-logic-acceptance/summary.json`
- Failure: Key modal did not open after clicking New key.
- Evidence: `keyModalState = { overlayClass: "hidden", children: 0, modal: false }`.

The same canonical script passed 17/17 when run inside Docker Playwright with Chromium revision 1243. Therefore the wrapper remains an unreliable harness.

### Required fix

Change `swarm/scripts/portal_logic_acceptance.sh` so the default path is Docker Playwright:

- image: `mcr.microsoft.com/playwright:v1.63.0-noble`
- `--network host`
- browser executable: `/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell`
- install `playwright@1.63.0` or `playwright-core@1.63.0` into the artifact dir with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`.

Keep local Playwright only as an explicit fallback when the local browser revision matches the package version.

## Issue 3 — harden selector and cleanup in canonical gate

One earlier Docker run failed because stale `mock-model` data from selfcheck made this selector match the model table under the picker:

```js
page.locator('text=mock-model').first().click()
```

Required fix:

- Scope picker selection to the picker modal/list, not global body text.
- Cleanup before and after must delete:
  - `mock-model`
  - `pw-*`
  - `crud-*`
  - `verify-*`
  - corresponding usage ledger rows and `http://127.0.0.1:9000/v1` backend rows.

## Do not mark done until these pass

DeepSeek should re-run:

```bash
cargo check --workspace
cargo test --workspace
bash swarm/scripts/portal_logic_acceptance.sh
```

Expected final state:

- `portal_logic_acceptance.sh` itself passes 17/17 without special manual Docker command.
- Custom LLM local blank key saves as `auth_mode=none`, even when `CUSTOM_LLM_API_KEY` exists in `.env`.
