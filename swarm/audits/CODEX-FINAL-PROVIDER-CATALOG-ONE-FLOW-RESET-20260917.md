# CODEX FINAL CONTRACT — Provider catalog + one-flow Model Route wizard

Date: 2026-09-17 14:50 ICT  
Role: Codex auditor/mentor. DeepSeek owns product code changes.  
Live DB was reset after this audit request.

## DB reset done now

Dev PostgreSQL was wiped and reseeded clean:

```text
usage_ledger = 0
api_keys     = 0
model_routes = 0
teams        = 1  (Default Team, unlimited budget)
backends     = 10 formal/custom rows, all disabled
```

Current clean provider rows for compatibility with existing UI:

```text
1  openai       https://api.openai.com                                    disabled
2  anthropic    https://api.anthropic.com                                 disabled
3  gemini       https://generativelanguage.googleapis.com/v1beta/openai   disabled
4  deepseek     https://api.deepseek.com                                  disabled
5  kimi         https://api.moonshot.ai/v1                                disabled
6  qwen         https://dashscope-intl.aliyuncs.com/compatible-mode/v1    disabled
7  zai          https://api.z.ai/api/paas/v4                              disabled
8  openrouter   https://openrouter.ai/api/v1                              disabled
9  meta-muse    https://api.meta.ai/v1                                    disabled
10 custom-llm   http://127.0.0.1:8088/v1                                  disabled
```

Router container was recreated and is healthy.

This reset is for dev cleanup. The product architecture below should stop relying on editable DB provider templates as the user's first step.

## Verdict

The current Portal still exposes the wrong mental model.

Admin should not have to create or edit a Provider first. Admin wants one thing:

> Create a public model route that calls an upstream model.

Therefore the primary flow must be:

```text
Models -> Create model route -> choose Provider -> key/url -> Load models -> choose one -> Test connection -> Save
```

No detour to Providers. No separate Provider Type decision. No hidden internal backend thinking.

## Simplified concepts

### What the user sees

- **Provider**: OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom LLM.
- **Provider URL**: shown/prefilled for known providers; editable only for Custom LLM unless Advanced is opened.
- **API key**: entered in the route wizard.
- **Provider model**: selected from loaded model list; exactly one model.
- **Public model name**: what teams call through BrighTO-Router.
- **Test connection**: required before saving enabled route.

### What remains internal

- `backends` table = connection row.
- `model_routes` table = public route.
- `protocol` / `auth_mode` = derived from provider choice, only editable under Advanced if needed.
- `format` = internal storage detail, never a primary UX concept.

## Provider type must be removed from normal UX

Do not show `Provider Type` as a required field in normal Provider/Route creation.

At most there are three internal API families:

```text
OpenAI-compatible
Anthropic Messages
Gemini/OpenAI-compatible endpoint if supported
```

But Admin should choose provider by name, not choose `Provider Type`.

Provider-specific defaults:

```text
OpenAI      -> OpenAI-compatible, Bearer, https://api.openai.com
DeepSeek    -> OpenAI-compatible, Bearer, https://api.deepseek.com
Kimi        -> OpenAI-compatible, Bearer, https://api.moonshot.ai/v1
Qwen        -> OpenAI-compatible, Bearer, non-token-plan base URL from config
Z.AI        -> OpenAI-compatible, Bearer, https://api.z.ai/api/paas/v4
OpenRouter  -> OpenAI-compatible, Bearer, https://openrouter.ai/api/v1
Anthropic   -> Anthropic Messages, x-api-key, https://api.anthropic.com
Gemini      -> disabled/coming soon until route protocol is fully tested
Meta Muse   -> disabled/experimental until valid base/key/model are known
Custom LLM  -> user fills URL; default OpenAI-compatible; auth can be Bearer or No API key
```

## Provider catalog belongs in .env/config, not user-created DB templates

The provider list should be a fixed catalog loaded from config/env, not something Admin must create before making a route.

Recommended `.env` shape:

```dotenv
OPENAI_BASE_URL=https://api.openai.com
ANTHROPIC_BASE_URL=https://api.anthropic.com
GEMINI_BASE_URL=https://generativelanguage.googleapis.com/v1beta/openai
DEEPSEEK_BASE_URL=https://api.deepseek.com
KIMI_BASE_URL=https://api.moonshot.ai/v1
QWEN_BASE_URL=https://dashscope-intl.aliyuncs.com/compatible-mode/v1
ZAI_BASE_URL=https://api.z.ai/api/paas/v4
OPENROUTER_BASE_URL=https://openrouter.ai/api/v1
META_MUSE_BASE_URL=https://api.meta.ai/v1
CUSTOM_LLM_BASE_URL=http://127.0.0.1:8088/v1

OPENAI_API_KEY=
ANTHROPIC_API_KEY=
GEMINI_API_KEY=
DEEPSEEK_API_KEY=
KIMI_API_KEY=
QWEN_API_KEY=
ZAI_API_KEY=
OPENROUTER_API_KEY=
META_MUSE_API_KEY=
CUSTOM_LLM_API_KEY=
```

Product rule:

- Route wizard reads provider catalog from server config/env/defaults.
- DB `backends` stores real connection rows created/reused by route creation.
- Fresh DB does not need editable provider template rows to let Admin create a route.
- Existing `backends` can stay for v1 compatibility, but the wizard must not require Admin to manage it manually.

Do not put complicated JSON provider catalog in README. Keep `.env` obvious and editable.

## Correct one-flow route creation

### Admin flow

```text
1. Admin -> Models
2. Create model route
3. Select Provider: DeepSeek
4. API key: paste key or use env default if present
5. Provider URL: prefilled, hidden unless Advanced/custom
6. Load models
7. Modal/list opens with models
8. Admin selects exactly one model
9. Public model name auto-fills, editable
10. Test connection
11. If pass, Save enabled becomes available
```

For Custom LLM:

```text
1. Admin -> Models
2. Create model route
3. Select Provider: Custom LLM
4. Base URL: http://127.0.0.1:8088/v1
5. Auth: No API key or Bearer
6. Load models or type model name
7. Select exactly one model
8. Test connection
9. Save enabled
```

## Load models must show a real chooser

Current behavior of typing/filling an input is not enough.

Required behavior:

- `Load models` opens a clear chooser panel/modal/table.
- Search/filter box if model list is long.
- Each row shows provider model name.
- User can select exactly one model.
- After selection, close chooser and fill `Provider model` + `Public model name`.
- If provider returns no list or list unsupported, show manual entry fallback.
- Never create a route just by clicking a model in a provider page.

Acceptance:

```text
Load models -> chooser visible -> select one model -> exactly one selected -> field filled
```

## Test connection is mandatory before enabled save

`Load models` is not a connection test.

Wizard must include:

```text
Test connection
```

It must make a real tiny upstream call using the current wizard values:

- provider URL
- provider API key or no-auth
- selected protocol/auth defaults
- selected provider model
- max output <= 8 tokens

Result shown:

```text
PASS: status 200, latency N ms
FAIL: sanitized reason
```

Save rules:

- `Save draft` is allowed without test and stores disabled route.
- `Save enabled` requires latest wizard values to have a passing test.
- If Provider / URL / Key / Auth / Protocol / Provider model changes, clear the pass state.

## Backends/connections should be created or reused automatically

When saving route:

1. Normalize base URL.
2. Find an existing backend/connection with same normalized URL + same internal format.
3. Reuse it if found.
4. Otherwise create one backend automatically.
5. Save route with selected backend id and route-level key reference.

Do not create duplicates for repeated Custom LLM routes to the same URL.

If route creation fails after auto-creating a backend and that backend has no usage/routes, clean it up.

## Providers page should become Connections page or Advanced Providers

This page is not the first step.

It should show inventory and safety controls:

- Provider/connection name.
- URL.
- Enabled/disabled.
- Active route count.
- Usage count.
- Actions: Edit URL/name, Enable/Disable, Create route from this connection, Delete only when safe.

Remove misleading status:

```text
Enabled · no key
```

Because key is route-level, provider/connection does not need a key. Better status:

```text
Template
Active · 2 routes
Disabled · affects 2 routes
Has usage · cannot delete
```

## Delete/disable lifecycle still applies

Keep the lifecycle contract already written:

- Provider cannot be deleted if it has active route or usage history.
- Provider can be disabled; associated routes become effectively disabled.
- Model route can be manually disabled.
- Model route cannot be deleted if it has transaction history.
- Provider and Model pages need filters: Enabled only / Disabled only / All.

## Acceptance tests required

### 1. Fresh DB one-flow DeepSeek

With empty routes/keys/usage and only catalog/default team:

1. Login Admin.
2. Go to Models.
3. Create model route.
4. Select DeepSeek.
5. Paste API key.
6. Load models.
7. Chooser opens.
8. Select `deepseek-v4-pro` or available DeepSeek model.
9. Test connection PASS.
10. Save enabled.
11. Client call `/v1/chat/completions` returns 200.

### 2. Fresh DB one-flow Custom LLM

1. Login Admin.
2. Go to Models.
3. Create model route.
4. Select Custom LLM.
5. Base URL `http://127.0.0.1:8088/v1`.
6. Auth `No API key`.
7. Load or type `qwen3.8-flash-next`.
8. Test connection PASS.
9. Save enabled.
10. Client call returns 200.

### 3. Duplicate prevention

1. Create Custom LLM route A with URL `http://127.0.0.1:8088/v1`.
2. Create Custom LLM route B with same URL.
3. DB has one backend/connection for that URL.
4. Both routes reference same backend id.

### 4. Provider list from env/config

1. Start with empty DB backends.
2. Portal still shows Provider choices in route wizard from config/env.
3. Creating a route creates/reuses backend automatically.

### 5. Test pass gating

1. Blank cloud API key -> Test connection blocked locally, no upstream 401/502 leak.
2. Bad key -> Test connection FAIL, Save enabled disabled.
3. Good key -> Test connection PASS, Save enabled enabled.
4. Change selected model after pass -> pass state clears.

## Current DB state after reset

DeepSeek can now implement against a clean DB. If it wants to test the future catalog-only design, it may truncate `backends` too; the new route wizard must still work from provider catalog in config/env.

Do not claim done until a new Admin can create a cloud route and custom-local route entirely from Models without visiting Providers.
