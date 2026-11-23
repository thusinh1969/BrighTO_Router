# Changelog

All notable changes to BrighTO Router are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow the `Cargo.toml` release line (`1.3.x`).

---

## [1.3.2] — 2026-11-23

Portal fix after a "select OpenRouter → Test model → Pick a provider model
first" report.

### Fixed

- **The model field is seeded as soon as a provider is chosen.** `suggestedModels()`
  returned an empty list for the chat family (only embedding/rerank/ASR/SystemOne
  had defaults), so picking OpenRouter/OpenAI/Anthropic/… left the provider-model
  input blank and `Test connection` dead-ended on *Pick a provider model first*.
  Chat suggestions now exist per provider (OpenRouter, OpenAI, Anthropic, Gemini,
  DeepSeek, Qwen, Kimi, Z.AI, Meta Muse); custom/local endpoints stay blank on
  purpose because their model names cannot be guessed.

- **`Test connection` no longer dead-ends.** With no model selected it now fetches
  the provider's own model list (`Load models`) and shows the picker, instead of a
  bare error.

- **The very first modal open also seeds the model.** The first `applyCat()` ran
  before `ms`/`mi` existed (`var` hoisting left them `undefined`), so the initial
  provider never populated the field; it is now re-run once the picker is wired up.

- **Modal context default raised to 1M** for chat/messages/completion/responses,
  matching the router default introduced in 1.3.1.

Portal is embedded in the binary (`include_str!`), so this ships as
`brighto-airouter:v1.3.0-gemini6`. Served HTML verified after deploy: chat
suggestions, test auto-load, first-open seed and the 1M default are all present.

### Performance — admin API was paying a full-table scan per request

A 23.4M-row `usage_ledger` with **no index** made every admin view slow, and the
dashboard could stay blank for ~37 s.

- **`usage_ledger` is now indexed** (`migrations/0011`): `(backend_id, model)`,
  `(ts)`, and `(ts, backend_id, model)`. The migration keeps plain `CREATE INDEX`
  so it stays transactional for the runner and `sqlx::test`; the live 23.4M-row
  table was built with `CREATE INDEX CONCURRENTLY` by hand so the router was
  never write-blocked. `VACUUM (ANALYZE) usage_ledger` is **required** afterwards:
  without a visibility map Postgres keeps fetching heap pages and the index-only
  scan ends up slower than the seq scan it replaced (measured 3.3 s vs 0.6 s).

- **`load_lifecycle_data()` no longer re-scans the ledger on every call.** It ran
  `SELECT backend_id, model, COUNT(*) FROM usage_ledger GROUP BY backend_id, model`
  for `backends`, `routes`, `teams` and `keys` alike (the numbers only feed the
  *"has N logged request(s)"* delete-blocker hint). Results are now cached for
  10 s. Measured on the live 23.4M-row table: `list_backends` 1.5 s → 0.09 s
  (cache hit), GROUP BY 1201 ms → 593 ms after indexing + vacuum.

- **Dashboard paints a skeleton immediately.** `renderDashboard` cleared
  `#content` and awaited `/admin/stats`, `/admin/usage` and `/admin/summary`, so
  a slow summary meant an empty page. The view now renders a shimmering
  placeholder first, and is replaced as soon as the data lands.

- **`/admin/summary` can no longer block the dashboard.** It aggregates the whole
  ledger (three `percentile_cont` sorts) and took 32–39 s cold. The call is capped
  at 2 s and falls back to the per-model stats already in hand; its result cache
  TTL went 45 s → 300 s. Warm latency 38.5 s → 0.055 s.

Dashboard first paint measured on the live portal: **~37 s → 5 s** warm
(up to ~12 s on the first load after a restart, while `/admin/summary` is still
cold), with the skeleton visible immediately. Full portal audit: 17/17 PASS,
0 page errors, 0 console errors, 0 admin 5xx.

Published as `thusinh1969/brighto_airouter:v1.3.2` (multi-arch: `linux/amd64` +
`linux/arm64`, also `:latest`). CI is green again: `cargo fmt --check`,
`cargo clippy --all-targets -D warnings` and 142 unit tests all pass, and the
release workflow's own Docker smoke now runs end to end.

---

## [1.3.1] — 2026-11-17

Follow-up hardening after a coding-agent field report: route edits silently
downgraded Gemini routes, tool signatures were lost on restart, and defaults
were too small for long-context coding work.

### Fixed

- **`PATCH /admin/routes/{name}` no longer resets omitted fields**
  (`src/admin/mod.rs`). `patch_route` only carried over 9 columns, so saving a
  route from the portal without resending `protocol` reset it to `openai_chat`.
  For a Gemini route that silently dropped the `gemini_chat` adapter (field
  normalisation **and** thought-signature handling), and the next tool call
  failed with `400 Function call is missing a thought_signature`. It now keeps
  every omitted scalar: `protocol`, `context_tokens`, `max_output_tokens`,
  `first_byte_timeout`, `chars_per_token`, `fallback_backend_id`,
  `price_input_per_mtok_usd`, `price_output_per_mtok_usd`, `enabled`,
  `auth_mode`, `routing_policy`, `provider_model_name` (`first_byte_timeout` is
  decoded as BIGINT and converted to the API's `u64`).

### Changed

- **`thought_signature` cache is now persistent and evicts oldest-first**
  (`src/gemini/thought.rs`). It is journalled to
  `$DATA_DIR/thought_signatures.jsonl` (mode 0600, compacted past 8192 lines) and
  replayed on boot, so a router restart no longer strands an in-flight agent
  turn. Eviction removes the oldest entry instead of clearing the whole map.
  - Unit test `eviction_is_oldest_first_and_bounded` covers the bound.
  - Verified live: record a tool call, restart the container, replay the same
    `tool_call_id` with no signature → `200`.

- **Default route limits raised** to **1M context / 256k output** for chat
  protocols (`src/admin/mod.rs`). Applies to routes created from now on; existing
  routes were backfilled explicitly.

### Verified (B300 production, `brighto-airouter:v1.3.0-gemini4`)

Coding-agent flow with a 23k-token system prompt, tools, several turns and text
interleaved between tool calls:

| Case | qwen4-max | qwen3.8-max-0902 | gemini-4-hydrogen |
|---|---|---|---|
| long-context tool call | 200 | 200 | 200 |
| multi-turn agent loop | 200 | 200 | 200 |
| `/v1/messages` tool_use | 200 | 200 (after enabling the bridge) | 200 |
| `/v1/messages` tool_result replay | 200 | 200 | 200 |
| parallel tool calls + replay | 200 | 200 | 200 |

Route limits corrected against upstream documentation:

| Route | context | max output | source |
|---|---|---|---|
| `gemini-4-argon` / `carbon` | 1,048,576 | 32,768 | Gemini `models.list` |
| `gemini-4-hydrogen` | 1,048,576 | 65,536 | Gemini `models.list` |
| `qwen4-max` | 1,000,000 | 131,072 | QwenCloud model page |
| `qwen3.8-max-0902` | 1,048,576 | 131,072 | probed: 131072 accepted, 262144 rejected |
| `google/gemma-4-31B-it` | 262,144 | 262,144 | HuggingFace model card (256K) |

### Removed

- Dead local Qwen routes `Qwen3.8-27B-Uncensored-NVFP4` and
  `Qwen/Qwen3.8-Flash-Next-NVFP4` (their vLLM processes are down), together with
  their `usage_ledger` history (5,429 + 541,464 rows) and route rows.
- `qwen3.8-max-0902` now accepts Anthropic Messages; the coding agent could not
  use it through `/v1/messages` before.

### Fixed (second pass — Anthropic bridge)

- **`/v1/messages` tool turns on Gemini routes lost their thought signature.**
  `forward_bridge_response` returns before the normal response path, so a
  Claude-CLI style call never cached the signature and the follow-up
  `tool_result` failed with `400 upstream returned HTTP 400`. The bridge now
  records signatures from the upstream OpenAI body — buffered JSON **and** SSE
  chunks — before translating.

### Final matrix (B300, `brighto-airouter:v1.3.0-gemini5`)

Coding-agent flow: 23k-token system prompt, tools, several turns and text
interleaved between tool calls.

| case | qwen4-max | qwen3.8-max-0902 | gemini-4-hydrogen |
|---|---|---|---|
| long-context tool call | PASS | PASS | PASS |
| multi-turn agent loop | PASS | PASS | PASS |
| `/v1/messages` tool_use | PASS | PASS | PASS |
| `/v1/messages` tool_result replay | PASS | PASS | PASS |

**12/12 PASS.** Cache persistence re-verified separately: record a tool call,
`docker restart`, replay the same `tool_call_id` without a signature → `200`.

---

## [1.3.0-gemini] — 2026-11-15

Operational release on top of `v1.3.0`: first-class Gemini support, DeepSeek-cloud
compatibility for local vLLM routes, and realistic thinking timeouts.

### Added

- **`gemini_chat` protocol — first-class Google Gemini support** (`src/gemini/mod.rs`).
  Google's OpenAI-compatible endpoint validates the payload strictly: an unknown
  field is a hard `400 Invalid JSON payload received. Unknown name "<x>": Cannot
  find field.`, and a JSON `null` still counts as present. The adapter removes
  exactly the fields Google rejects — `store`, `seed`, `logprobs`, `top_logprobs`,
  `frequency_penalty`, `logit_bias`, `metadata`, `prediction`,
  `web_search_options` — and drops `max_tokens` when `max_completion_tokens` is
  present (Gemini refuses both together).
  - Routes declare `protocol: gemini_chat`; there is no per-route field list.
  - `gemini_chat` belongs to the OpenAI-chat family, so the Anthropic bridge
    (`accept_anthropic_messages`, Claude CLI / Claude Code) works unchanged.
  - Field table verified live against `gemini-4-hydrogen`; covered by 5 unit tests.
  - Routes created: `gemini-4-argon`, `gemini-4-carbon`, `gemini-4-hydrogen`.

- **DeepSeek-cloud compatibility for local vLLM routes** (`src/compat/mod.rs`,
  requirement: `LLM_guide_needed.md`). An OpenAI-chat route flagged
  `deepseek_compat` now behaves like `api.deepseek.com` for the owner's harness:
  - request: `thinking{type}`, `reasoning_effort`, `reasoning.effort` and
    `output_config.effort` are translated into `chat_template_kwargs`
    (`thinking`, `reasoning_effort`) and removed from the forwarded body;
    effort `none` disables thinking, integer effort is clamped to 1..100.
  - assistant history: `reasoning` + `reasoning_content` collapse into a single
    `reasoning_content` (vLLM answers `400 duplicate field 'reasoning'` when both
    are sent back with tool calls).
  - response: `message`/`delta.reasoning_content` added next to vLLM's
    `reasoning` (non-stream and every SSE chunk), plus
    `usage.prompt_cache_{hit,miss}_tokens`.
  - 15 unit tests; acceptance F1–F5 verified on gx10 and B300.

- **`CHANGELOG.md`** (this file) so the team can follow releases.

### Changed

- `first_byte_timeout` raised to **900 s** for the local vLLM thinking routes
  (`deepseek-v41-flash`, `Qwen/Qwen3.8-Flash-Next-NVFP4`,
  `Qwen3.8-27B-Uncensored-NVFP4`) via migration `0010`. Non-streaming
  high-effort calls exceeded the old 180 s and the router answered
  `HTTP 504 first byte timeout` (29 of 34 audit errors). The router polls
  `model_routes` every 5 s, so no restart was needed.
  Streaming is unaffected (first byte arrives early).

### Migrations

| File | Purpose |
|---|---|
| `0009_deepseek_compat_model_card.sql` | nullable model-card columns on `model_routes` (`display_name`, `owned_by`, `input_modalities`, `output_modalities`, `effort_levels`, `effort_default`, `system_prompt_update`, `deepseek_compat`) + seed for `deepseek-v41-flash` |
| `0010_raise_first_byte_timeout_thinking_routes.sql` | 900 s first-byte timeout for the local vLLM thinking routes |

Both are additive and safe for the previous binary.

### Tool calling — thought_signature carry-over

Gemini binds every returned `functionCall` to an opaque `thought_signature`
(`tool_calls[].extra_content.google.thought_signature`) and **requires it back**
for the most recent assistant tool call:

```text
400 Function call is missing a thought_signature in functionCall parts.
```

Generic OpenAI clients rebuild `tool_calls` from id/name/arguments and drop
`extra_content`, so the signature is lost. `src/gemini/thought.rs` keeps a small
bounded cache (`tool_call_id -> signature`, 4096 entries): signatures are
recorded from every Gemini response (non-stream body, chunked body and SSE
deltas) and re-injected into the last assistant tool call of the next request.
Only the last call is touched — an older tool call followed by another assistant
answer needs no signature (verified). Covered by 4 unit tests.

### Verified on B300 (production `:8000`)

- 3 Gemini routes, `/v1/chat/completions` **and** `/v1/messages`:
  `200 "OK"` even while sending `store`, `seed`, `logprobs`, `top_logprobs`,
  `frequency_penalty`, `metadata` in the same request.
- **Tool round trip**: first turn returns the tool call with its signature; the
  follow-up replaying `tool_calls` **without** `extra_content` (what the harness
  sends) is `200` — the router re-attaches the signature.
- Streaming (`SSE`), streaming tool call, long output (`max_tokens=8192`),
  Anthropic `/v1/messages` with tools: all `200`.
- No regression: 22 routes intact, embeddings/rerank/systemone unchanged.

### Rollback

- Previous image: `brighto-airouter:v1.3.0` (same binary as before this release).
- DB: restore the pre-migration dump, or leave the additive columns in place —
  the `v1.3.0` binary ignores them.

---

## [1.3.0] — 2026-10-27

Claude CLI bridge and Qwen dual protocol.

### Added

- Anthropic Messages ↔ OpenAI Chat bridge for LLM routes
  (`accept_anthropic_messages` on an OpenAI-chat route; `/v1/messages` served by
  an OpenAI backend).
- Migration `0008` (`accept_anthropic_messages` column) plus upgrade/idempotency
  tests.
- Portal toggle and Claude CLI bridge badge.
- Isolated HTTPS Qwen 3.8 UAT (single public model).

### Fixed

- Model connection probes and context-limit metadata UX.
- Test connection no longer sends `max_tokens`, so reasoning models
  (`o1`/`o3`/`o4`) stop returning 400.
- Cloud model discovery and output-limit metadata.

---

## [1.2.0] — 2026-10-26

Security and correctness hardening.

### Security

- Admin auth: constant-time key comparison, per-source-IP failure rate limiting
  (10 failures / 60 s → `429`) and audit logs.
- CSP, `no-store`, frame and referrer protections; client keys are revealed only
  on an explicit request.
- TLS private keys stay owner-only; the non-root router container gets read
  access through a POSIX ACL.

### Fixed

- Health probes cover LAN hostnames; unsupported probe endpoints are reported as
  `unknown` instead of opening the circuit.
- Large non-stream requests are buffered and rewritten for aliased routes.
- Usage model/status/provider/team/key/date filters are applied to stats and logs.

### Performance

- `/admin/summary` result cache (45 s TTL) so the dashboard reload is instant;
  cold aggregation over ~1.2 M ledger rows drops from ~3.2 s to ~2.5 s with
  PostgreSQL tuning (`shared_buffers=4GB`, `work_mem=64MB`, 8 parallel workers).

---

## [1.1.0] and earlier

See `git log --oneline` (the repository history predates this changelog).
