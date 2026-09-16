# CODEX audit — Portal product gap before Playwright gate — 2026-09-17

Role: Codex auditor only. No implementation code changed.

Current worktree inspected:

```text
M  src/admin/mod.rs
M  static/index.html
?? migrations/0003_admin_key_reveal.sql
```

## What passes now

These checks passed against the current uncommitted DeepSeek worktree:

```bash
node --check /tmp/brighto-current-portal.js
python3 scripts/hotpath_guard.py
cargo check --locked --all-targets
./scripts/test_postgres.sh
cargo build --release --locked
python3 scripts/portal_smoke.py
```

Result:

```text
HOTPATH_GUARD_PASS
cargo check PASS
Rust tests PASS: 55 unit + 4 integration
portal_smoke.py PASS
```

Extra runtime reveal audit done by Codex with temporary PostgreSQL + release binary:

```text
POST /admin/keys              -> 200, returns lc- key
GET /admin/keys/{id}/reveal   -> 200, returns same plaintext key
legacy/null key_secret reveal -> 410, clear recreate message
bad admin reveal              -> 401
RESULT PASS
```

Backend reveal direction is acceptable for the latest user override: Admin may view BrighTO client API keys again.

## Do not run Playwright gate yet

Current Portal is not ready for Playwright acceptance because it still misses required product flows. Running Playwright now would only prove incomplete UI. Finish the gaps below first, then Codex will run the real browser all-click audit from `CODEX-PLAYWRIGHT-AUDIT-PROTOCOL-20260917.md`.

## Must fix before saying "Portal done"

### 1. API key smoke script is stale

`scripts/portal_smoke.py` still says and tests the old rule:

```text
Create a client key (plaintext returned once)
admin /keys never returns plaintext
```

Keep the `/admin/keys` list safe: it should not return plaintext secrets in every row. But the smoke must now also verify the explicit reveal endpoint:

- create API key;
- `GET /admin/keys/{id}/reveal` returns the same key;
- key with `key_secret IS NULL` returns HTTP 410 and a clear message;
- bad admin key cannot reveal;
- user/client API key cannot call admin reveal.

This prevents future regressions against the user override.

### 2. Remove confusing "one copy" wording in Portal

`static/index.html` currently shows:

```text
This is the only plaintext copy unless you re-open it later via View key.
```

That is technically contradictory and weak product copy. Replace with simple Admin language:

```text
Admin can view and copy this client API key later from the API Keys table.
```

Scope remains narrow: this applies only to BrighTO client API keys. Provider LLM API keys must not be revealed in Portal unless user gives a separate explicit order.

### 3. Model Route flow is not the requested product yet

User requirement for route creation:

```text
chọn Provider, cho API, max tokens hay lấy từ model, load model name và chọn 1 model name duy nhất và giá tiền 1M in, 1M out, lưu lại
```

Current UI only covers model name, backend(s), fallback, and first-byte timeout. There is no field for:

- provider API key setup/test inside the route flow or clearly linked provider flow;
- one selected provider model name after loading provider models;
- context/max tokens;
- price per 1M input tokens;
- price per 1M output tokens;
- currency/units shown as USD per 1M tokens.

Root cause is likely schema + UI mismatch. Minimal production schema should add route/provider metadata needed for cost display and admin clarity, for example:

```text
provider_model_name
context_tokens
max_output_tokens
price_input_per_mtok_usd
price_output_per_mtok_usd
```

Keep it simple. Do not add Redis, billing engine, pricing sync service, or enterprise subscription objects for this community version.

### 4. API Key form does not expose production controls

Backend already has fields for:

```text
budget
rpm_limit
concurrency_limit
expires_at
allowed_models
enabled
```

Portal create-key modal currently exposes only:

```text
team
owner
allowed models
```

Add straightforward fields:

- expiry date/time or no expiry;
- key budget: inherit team / unlimited / money / token budget;
- rate limit: requests per minute, optional;
- concurrency limit, optional;
- enabled status after create.

Admin should be able to see these values in the API Keys table without opening raw JSON.

### 5. Team budget UI is too raw

Current team budget is raw JSON textarea. This is acceptable for internal dev, but not for a professional open-source first run.

Replace or wrap it with simple choices:

```text
Unlimited
Monthly token budget
Monthly money budget
Daily token budget
Daily money budget
```

If raw JSON is kept, hide it behind an "Advanced JSON" disclosure. Default user flow must not require JSON knowledge.

### 6. Usage dashboard lacks required filters and cost meaning

User asked dashboard usage by:

```text
provider, model, team, key, full system
```

Current Usage screen shows token chart and logs, but no visible filters. Required minimum:

- date range;
- provider/backend;
- model;
- team;
- API key;
- status class: all / success / error;
- totals update after filter: requests, input tokens, output tokens, estimated cost if route price exists.

If price columns are not implemented yet, label cost as unavailable instead of showing fake numbers.

### 7. Provider setup should be first-run friendly

Provider table is close, but first-run UX still needs to match the install story:

- default providers appear immediately after seed;
- each provider row clearly shows `Configured` or `Missing API key`;
- `Add provider` can create Custom provider with name + base URL + format;
- `Load models` gives a clear error when key/base URL is missing;
- no provider secret is printed back to the browser.

### 8. Settings screen is acceptable but should show source of truth

`GET /admin/settings` is useful. Add only small clarity if needed:

- database connected/unreachable;
- config reload healthy/stale;
- router address;
- version;
- admin CIDR summary if safe to show.

No complex settings write UI now. Environment + restart is fine for community version.

## Acceptance gates after fixes

DeepSeek should run these before handing back:

```bash
node --check /tmp/brighto-current-portal.js
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo check --locked --all-targets
./scripts/test_postgres.sh
cargo build --release --locked
python3 scripts/portal_smoke.py
```

Then say explicitly:

```text
Portal product flows ready for Codex Playwright audit.
```

Codex will then run the real Playwright audit: all steps/windows/clicks, desktop + mobile, screenshots, console/network failures, and `summary.md` under `swarm/out/playwright/<timestamp>/`.
