# Portal SOTA product design - 2026-09-17

User verdict: current Portal is not acceptable. Build the product design first, then code.

This is the target design for DeepSeek.

## Product position

BrighTO-Router Portal is a professional control plane for a team LLM gateway.

A first-time user should open one URL, log in as admin, connect LLM providers, choose exact models, set prices and limits, create teams and API keys, then watch usage by provider, model, team, and full organization.

Do not copy OpenRouter. Match the level of clarity and polish: clean dark dashboard, dense but readable tables, obvious primary actions, no raw JSON as the main user experience.

## First screen: admin login

URL `/` shows only a polished login page until authenticated.

Fields:

```text
Username: admin
Password: value of ADMIN_MASTER_KEY from .env
```

Default local password:

```text
brightoIsGreat@2026
```

Implementation rule for open-source version:

- Do not add an admin-users table yet.
- Treat username `admin` plus password `ADMIN_MASTER_KEY` as the login credential.
- Store the admin token in browser `sessionStorage`, not in localStorage by default.
- Provide a visible logout button.
- If login fails, show one clear message: `Invalid admin username or password.`

Enterprise version later can add SSO and admin user management. Do not add SSO now.

## Dashboard after login

After login, route to dashboard view in the same static app.

Navigation:

```text
Dashboard
Providers
Models & Routes
Teams
API Keys
Usage
Settings
```

Top cards:

```text
Total requests today
Total input tokens
Total output tokens
Estimated spend
Active providers
Active teams
Error rate
P95 first byte time
```

Main charts:

1. Tokens over time, split input/output.
2. Spend over time.
3. Usage by provider.
4. Usage by model.
5. Usage by team.

Main tables:

1. Recent requests: time, team, key prefix, model, provider, status, input tokens, output tokens, cost, first byte time, total time.
2. Top models: model, provider, requests, tokens, spend, error rate.
3. Top teams: team, requests, tokens, spend, budget remaining.

Terminology in UI:

- `Input tokens`: tokens sent to a model.
- `Output tokens`: tokens returned by a model.
- `First byte time`: time until the first response byte reaches the client.
- `Total time`: full request duration.
- `Spend`: estimated cost from configured model prices.

No unexplained abbreviations on the page.

## Providers screen

Purpose: maintain provider templates and custom provider endpoints.

Default provider rows must be easy to see and edit:

```text
OpenAI
Anthropic
Gemini
DeepSeek
Kimi
Qwen
Z.AI
OpenRouter
Meta Muse
Custom OpenAI-compatible
```

Provider fields:

```text
Name
Base URL
Format: OpenAI-compatible or Anthropic Messages
API key status: configured / missing
Enabled: yes / no
Max concurrent requests, optional
```

Primary actions:

```text
Add provider
Edit provider
Enable / disable
Test connection
Load models
```

API key UX:

- Admin may paste the provider API key in Portal.
- Backend must never return plaintext provider key.
- Storage choice must stay simple and production-safe:
  - open-source default can write provider keys to `.env` via an admin endpoint, or keep current `env:NAME` references and show the exact `./start.sh set-key` command;
  - if Portal stores keys, store only encrypted/file/env references, not plaintext in PostgreSQL.
- UI must show only `configured` or `missing`.

Do not require users to edit SQL or manually insert database rows.

## Models & Routes screen

This is the most important setup flow.

User flow:

1. Click `Create model route`.
2. Choose one Provider.
3. Enter or confirm Provider API key.
4. Click `Load models`.
5. Select exactly one model name from the provider response, or type one manually if the provider does not expose a compatible model-list endpoint.
6. Set model context limit:
   - default from provider metadata if available;
   - otherwise manual `Max tokens/context window` field.
7. Set pricing:
   - price per 1M input tokens;
   - price per 1M output tokens;
   - currency default USD.
8. Save.

Route fields:

```text
Public model name shown to clients
Provider
Provider model name
Max tokens / context window
Price per 1M input tokens
Price per 1M output tokens
Enabled
```

Keep fallback out of the first UI path. The backend can support multiple backend IDs, but the Portal should start with one selected provider/model to avoid confusion.

Advanced section, collapsed by default:

```text
Fallback provider
First byte timeout
Chars per token estimate
Max in-flight requests
```

Validation:

- Provider is required.
- Model name is required.
- Price fields must be zero or positive numbers.
- Max tokens must be empty or a positive integer.
- Saving route must reload router config immediately before showing success.

Current backend gap:

Existing `model_routes` is not enough for the requested product because it lacks:

```text
provider model name
context/max tokens
price per 1M input tokens
price per 1M output tokens
currency
enabled flag
```

Minimal schema path:

```text
Add columns to model_routes:
provider_model_name TEXT
max_context_tokens BIGINT
price_input_per_million NUMERIC
price_output_per_million NUMERIC
currency TEXT DEFAULT 'USD'
enabled BOOLEAN DEFAULT TRUE
```

Alternative: add `provider_models` table only if the implementation needs to cache the provider model list. Do not add it just for UI display if direct fetch is enough.

## Teams screen

Purpose: create teams and set team-level budget.

Fields:

```text
Team name
Enabled
Budget type: Unlimited / Money / Tokens
Budget period: day / month
Money budget amount
Token budget amount
Allowed models, optional
```

Default:

```text
Budget type: Unlimited
Enabled: yes
```

UX rules:

- `Unlimited` should be explicit, not an empty confusing JSON field.
- Team budget should show remaining amount when usage exists.
- If a team is disabled, all keys under that team must stop working.

Backend note:

Existing `Budget` supports tokens. To support money cleanly, extend budget JSON with money fields while preserving token fields:

```json
{
  "period": "month",
  "max_tokens": 1000000,
  "max_usd_cents": 5000,
  "per_model": {}
}
```

A missing `max_tokens` and missing `max_usd_cents` means unlimited.

## API Keys screen

Purpose: issue keys to users/services under a team.

Fields:

```text
Team
Owner name/email
Allowed models, optional
Expiry: never / date-time
Budget type: inherit team / unlimited / money / tokens
Money budget amount
Token budget amount
Requests per minute, optional
Concurrent requests, optional
Enabled
```

UX rules:

- Plaintext API key is shown only once after creation.
- After creation, list only prefix, owner, team, allowed models, expiry, budget, enabled state.
- Add clear copy button on key creation.
- Add disable/revoke action.
- No plaintext key appears in tables, logs, screenshots, or API responses after creation.

## Usage screen

Filters:

```text
Date range
Provider
Model
Team
API key prefix
Status code
Streaming: all / streaming / non-streaming
```

Charts:

```text
Tokens over time
Spend over time
Requests over time
Errors over time
First byte time over time
```

Tables:

```text
Recent requests
By provider
By model
By team
By API key prefix
```

Each row should include:

```text
requests
input tokens
output tokens
total tokens
estimated spend
average first byte time
average total time
error count
```

Current backend gap:

Existing ledger stores tokens and timings, but not provider name/model price at the time of request. For spend dashboards and money budgets, add request-time cost fields so historical costs do not change when prices are edited later.

Minimal ledger extension:

```text
provider_name TEXT
provider_model_name TEXT
price_input_per_million NUMERIC
price_output_per_million NUMERIC
cost_usd NUMERIC
```

This must be filled from the RAM snapshot in the hot path and written asynchronously through the existing ledger sink. Do not query PostgreSQL on the request path.

## Settings screen

Show read-only runtime settings first:

```text
Router address
Database status
Config reload status
Max body bytes
Config poll seconds
Docker image tag
Version/commit if available
```

Editable settings should be limited to admin-safe values. Avoid turning this into a full config editor.

## Required backend/API endpoints

Already present or mostly present:

```text
GET /admin/backends
PATCH /admin/backends/{id}
GET /admin/backends/{id}/models
GET /admin/routes
POST /admin/routes
GET /admin/teams
POST /admin/teams
PATCH /admin/teams/{id}
GET /admin/keys
POST /admin/keys
DELETE /admin/keys/{id}
GET /admin/usage
GET /admin/stats
GET /portal/me
GET /portal/me/usage
GET /portal/me/stats
```

Needed for the target design:

```text
POST /admin/backends
POST /admin/backends/{id}/test
POST /admin/backends/{id}/key or equivalent provider-key update flow
PATCH /admin/routes/{model_name} or keep POST /admin/routes as upsert but document it clearly
DELETE /admin/routes/{model_name}
GET /admin/stats with filters: provider, model, team, key, from, to
```

Avoid adding many endpoints if one simple upsert/list endpoint is enough.

## Visual direction

Use these references:

```text
swarm/docs_builds/visuals/github-banner.png
swarm/docs_builds/visuals/portal-admin-mock.png
swarm/docs_builds/visuals/portal-user-mock.png
swarm/docs_builds/visuals/benchmark-visual.png
```

The UI should feel like a modern infra/SaaS dashboard:

- dark background;
- clear cards;
- tight tables;
- strong primary buttons;
- readable empty states;
- no raw JSON unless in an advanced editor;
- mobile usable, desktop optimized.

## Acceptance checklist

Do not call the Portal done until all pass:

```text
Open / -> login page only
Login with admin / ADMIN_MASTER_KEY -> dashboard
Bad admin password -> clean error
Dashboard loads providers, routes, teams, keys, and stats
Can add/edit provider by name + URL
Can set provider API key without plaintext leaking back
Can load provider models
Can create one model route with provider, model, max tokens, 1M input price, 1M output price
Can create team with unlimited/money/token budget
Can create API key with expiry and own money/token budget or inherit team budget
Plaintext API key shown once only
Usage dashboard can filter by provider/model/team/key/date
User mode with BrighTO API key can see only own key/team/usage
Bad user API key returns 401
No PostgreSQL read in hot path
No Redis added
No heavy frontend build unless user explicitly accepts it
```

Required validation commands after implementation:

```bash
node --check /tmp/extracted-portal.js
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
./scripts/test_postgres.sh
cargo build --release --locked
python3 scripts/portal_smoke.py
```

Add or extend smoke tests for:

```text
admin login flow assumptions
provider create/edit/test
model route create with price and max token fields
team money/token/unlimited budget
API key budget/expiry
stats grouped by provider/model/team/key
no plaintext provider or client key leaks
```

## Non-goals for this pass

Do not add these now:

```text
SSO
multi-admin user database
Redis
semantic cache
prompt logging
provider billing reconciliation
complex organization hierarchy
React/Vite/Tailwind migration without explicit approval
```

The enterprise version can later add SSO, audit log, role-based access, managed secrets, advanced analytics, and organization-level policy.
