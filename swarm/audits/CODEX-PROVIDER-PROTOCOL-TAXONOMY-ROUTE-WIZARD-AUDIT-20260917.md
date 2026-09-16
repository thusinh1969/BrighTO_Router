# CODEX AUDIT — Provider protocol taxonomy for SOTA route wizard

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Current provider protocol model is still too coarse.

`openai` vs `anthropic` is not enough for a professional router UI. OpenAI itself has multiple API shapes, and many providers say “OpenAI-compatible” while only supporting a subset.

DeepSeek must redesign the route wizard around explicit provider protocol/capability, not a vague “format” field.

## Current gap

Observed code/product state:

- `BackendFormat` only has `OpenAi` and `Anthropic`.
- Provider form still exposes `Format (openai / anthropic)`.
- Route wizard has auth mode but not a clear protocol/capability choice.
- README/PROVIDERS use “OpenAI-style” too broadly.
- `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, and `/v1/messages` are all router routes, but provider capability is not selected per route.

This causes real setup mistakes:

- OpenAI Chat Completions and OpenAI Responses are different APIs.
- OpenAI-compatible providers often support Chat Completions but not Responses.
- Some providers expose `/v1/models`; some do not.
- Gemini may be used through OpenAI-compatible endpoint or native Gemini API.
- Anthropic Messages uses a different request/response/usage shape and auth header.
- Embeddings route is not the same as chat route.
- Local llama.cpp/vLLM may be OpenAI-compatible chat but no-auth.

## Design rule

Use two separate concepts:

1. Provider template: default base URL, display name, supported protocol presets.
2. Model route: one public model, one provider model, one protocol preset, one auth mode, one credential/no-auth setting.

Do not ask users to reason about internal enum names like `openai`. Show practical choices.

## Required protocol presets

Use these minimum protocol presets in the route wizard.

| UI label | Internal value | Incoming client path | Upstream path | Auth default | Model list | Body transform |
|---|---|---|---|---|---|---|
| OpenAI Chat Completions | `openai_chat` | `/v1/chat/completions` | `/v1/chat/completions` | Bearer | `/v1/models` `data[].id` | rewrite top-level `model` if needed |
| OpenAI Completions | `openai_completions` | `/v1/completions` | `/v1/completions` | Bearer | `/v1/models` `data[].id` | rewrite top-level `model` if needed |
| OpenAI Embeddings | `openai_embeddings` | `/v1/embeddings` | `/v1/embeddings` | Bearer | `/v1/models` `data[].id` or manual | rewrite top-level `model` if needed |
| OpenAI Responses | `openai_responses` | `/v1/responses` | `/v1/responses` | Bearer | `/v1/models` `data[].id` | future/optional; do not claim supported until endpoint exists |
| Anthropic Messages | `anthropic_messages` | `/v1/messages` | `/v1/messages` | x-api-key | manual or provider-specific | request body already Anthropic shape |
| Local OpenAI-compatible Chat | `local_openai_chat` | `/v1/chat/completions` | `/v1/chat/completions` | none by default | `/v1/models` `data[].id` | rewrite top-level `model` if needed |
| Custom OpenAI-compatible | `custom_openai_chat` | `/v1/chat/completions` by default | configurable OpenAI-style path | Bearer or none | configurable/manual | rewrite top-level `model` if needed |

Do not show `openai_responses` as enabled unless `/v1/responses` is actually implemented and tested.

## Provider template defaults

Seed templates should provide friendly defaults, but the route decides final protocol/auth.

| Provider | Default base URL | Suggested presets |
|---|---|---|
| OpenAI | `https://api.openai.com` | OpenAI Chat Completions, OpenAI Responses, OpenAI Embeddings |
| Anthropic | `https://api.anthropic.com` | Anthropic Messages |
| Gemini OpenAI-compatible | `https://generativelanguage.googleapis.com/v1beta/openai` | OpenAI Chat Completions, OpenAI Embeddings if verified |
| DeepSeek | `https://api.deepseek.com` | OpenAI Chat Completions |
| Kimi / Moonshot | `https://api.moonshot.ai/v1` | OpenAI Chat Completions |
| Qwen / DashScope compatible | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | OpenAI Chat Completions, maybe Embeddings if verified |
| Z.AI | current configured base URL | OpenAI-compatible only if verified |
| OpenRouter | `https://openrouter.ai/api/v1` | OpenAI Chat Completions |
| Meta Muse | configured base URL | mark experimental/manual unless verified |
| Local llama.cpp/vLLM | `http://127.0.0.1:8088/v1` or custom | Local OpenAI-compatible Chat |
| Custom | empty/manual | Custom OpenAI-compatible or Anthropic Messages |

Every seeded provider should show “verified protocol presets” vs “manual/experimental”. Do not imply all providers support all routes.

## Route wizard fields

The route wizard must be a guided flow, not a bag of checkboxes.

### Step 1 — Provider

- Select provider template.
- If Custom/Local, edit base URL.
- Show detected base URL plainly.

### Step 2 — Protocol

Dropdown label: “Provider API protocol”.

Options are filtered by provider template:

- OpenAI Chat Completions
- OpenAI Completions
- OpenAI Embeddings
- Anthropic Messages
- Local OpenAI-compatible Chat
- Custom OpenAI-compatible

If a protocol is not implemented, hide it or mark disabled with exact reason. Do not show dead options.

### Step 3 — Authentication

Dropdown label: “Provider authentication”.

Options:

- Bearer API key
- Anthropic x-api-key
- No API key, local/private server

Rules:

- OpenAI/compatible default: Bearer API key.
- Anthropic Messages default: Anthropic x-api-key.
- Local OpenAI-compatible default: No API key.
- If No API key is selected, provider key field is hidden/disabled and backend sends no `Authorization` or `x-api-key` upstream.

### Step 4 — Load models

Load models must use current wizard values before save:

- base URL;
- protocol;
- auth mode;
- typed provider key, if any.

Do not require provider-level stored key. Route owns credential.

If model list fails, show:

- HTTP status;
- endpoint attempted;
- auth mode used;
- suggestion: type provider model manually if provider does not expose `/models`.

Do not show provider key.

### Step 5 — Select one provider model

Exactly one model. No checkbox list.

Allow manual entry only when model fetch is unavailable.

### Step 6 — Public route

- Public model name defaults to provider model name.
- Admin may rename it.
- Explain: clients call public name; router forwards provider model name upstream.

### Step 7 — Limits/pricing

- Context tokens.
- Max output tokens.
- Price per 1M input tokens.
- Price per 1M output tokens.
- Timeout.
- Enabled.

## Runtime contract

Add a route-level `protocol` field separate from auth mode.

Suggested enum:

```rust
pub enum ProviderProtocol {
    OpenAiChat,
    OpenAiCompletions,
    OpenAiEmbeddings,
    OpenAiResponses,
    AnthropicMessages,
}
```

Keep auth separate:

```rust
pub enum ProviderAuthMode {
    Bearer,
    AnthropicXApiKey,
    None,
}
```

Do not overload `BackendFormat` to mean protocol, body shape, endpoint, and auth all at once.

## Routing rules

A route should declare which incoming endpoint it supports.

Examples:

- route `deepseek-v4-pro` protocol `openai_chat` accepts `/v1/chat/completions` only;
- embedding route accepts `/v1/embeddings` only;
- Anthropic route accepts `/v1/messages` only.

If a client calls a public model on the wrong endpoint, return a clear `400` or `404`:

```json
{
  "error": {
    "message": "model route is configured for OpenAI Chat Completions, not embeddings"
  }
}
```

Do not forward wrong endpoint shapes upstream and let providers fail confusingly.

## Model-list rules

OpenAI-compatible model list parser:

- endpoint: join base URL with `/v1/models` using existing SDK-base-url logic;
- parse `data[].id`.

Anthropic model list:

- Do not assume if not implemented/tested.
- Either manual model entry, or implement provider-specific model-list endpoint only with a real test.

Gemini native:

- Do not claim native Gemini support unless request/response transform is implemented and tested.
- Today Gemini OpenAI-compatible endpoint is acceptable if documented as OpenAI-compatible.

OpenRouter:

- Model list may include many provider-prefixed IDs. UI must support search/filter, not giant chips.

Local llama.cpp/vLLM:

- use `/v1/models`;
- no-auth must send no auth headers;
- default URL suggestion can be `http://127.0.0.1:8088/v1`.

## Request body rewrite rules

OpenAI-style JSON:

- Rewrite top-level `model` only.
- Keep messages/input untouched.
- For large streaming upload, avoid full body parse; either prefix-safe rewrite or require public model == provider model.

Anthropic Messages:

- Anthropic request body may not use a top-level `model` exactly like OpenAI-compatible calls depending on client path.
- If router exposes Anthropic-compatible `/v1/messages`, validate and rewrite only the Anthropic top-level model field.
- Parse usage using Anthropic response shape, already partly supported.

Embeddings:

- Rewrite top-level `model`.
- Track usage if provider returns embedding usage.
- Do not show as chat route in UI.

## Docs wording to fix

Replace vague “OpenAI-style” where it hides important differences.

Use:

- OpenAI Chat Completions compatible
- OpenAI Embeddings compatible
- Anthropic Messages compatible
- Local OpenAI-compatible chat server
- Custom OpenAI-compatible endpoint

README should not claim `/v1/responses` until implemented.

PROVIDERS.md should include a protocol matrix, not only base URL and key env.

## Acceptance tests

DeepSeek must add/run these tests:

### Protocol endpoint guard

1. Create chat route.
2. Call `/v1/chat/completions` with that model -> forwards.
3. Call `/v1/embeddings` with that model -> clear error, no forward.

### Embeddings route

1. Create embedding route.
2. Call `/v1/embeddings` -> forwards to provider embedding endpoint.
3. Usage recorded under public model.

### Anthropic route

1. Create Anthropic Messages route.
2. Call `/v1/messages`.
3. Upstream receives `x-api-key` and `anthropic-version`.
4. Usage parser records input/output tokens.

### Local no-auth chat

1. Create Local OpenAI-compatible Chat route with auth none.
2. Load models from llama.cpp/vLLM.
3. Call `/v1/chat/completions`.
4. Upstream receives no auth headers.

### Public/provider model rewrite

1. Public model `qwen-local`.
2. Provider model `qwen3.8-flash-next`.
3. Upstream receives `qwen3.8-flash-next`.
4. Usage/dashboard records `qwen-local`.

### UI route wizard

Playwright must verify:

1. Provider dropdown filters protocol choices.
2. Protocol dropdown changes auth defaults.
3. No-auth hides provider key input.
4. Load models uses unsaved wizard key/no-auth.
5. User selects exactly one model from searchable dropdown.
6. Save route works.
7. Edit route keeps original identity.

## Keep it simple

Do not build a universal provider-transformation engine now.

Minimum SOTA for open-source release:

- OpenAI Chat Completions compatible;
- OpenAI Embeddings compatible if current endpoint is truly forwarded/tested;
- Anthropic Messages compatible;
- Local OpenAI-compatible chat no-auth;
- manual model entry fallback.

Everything else should be labeled “future” or “manual custom endpoint”, not half-supported.
