# What the Router must do so that no shim is needed (DeepSeek-cloud compatibility for local vLLM routes)

Written 2026-11-01 from the working shim `/home/dev/steve/deepseek_v41_flash_1017/shim/ds_cloud_shim.py` (about 130 lines, Python/aiohttp, container `ds-cloud-shim`, port 8094 in front of vLLM :8093). Every item below was tested on that shim unless marked otherwise. Cloud reference: https://api-docs.deepseek.com/guides/thinking_mode and https://api-docs.deepseek.com/api/list-models

Why: the owner's DeepSeek agent harness works against DeepSeek cloud. Against the router it gets no effort picker (the router's `/v1/models` lacks the `effort` field), the local model's thinking comes back under another field name, and the cloud effort/thinking parameters have no effect on vLLM.

Route concerned: `deepseek-v41-flash` (backend 28 = vLLM 0.31.0, `http://10.206.136.7:8093/v1`, served model name `deepseek-v41-flash`). Apply the request/response rewrites below only to routes whose backend is vLLM, not to the cloud backends (cloud routes already behave correctly).

## A. `GET /v1/models` and `GET /v1/models/{id}`: return the cloud model object

Today every route returns only `{"id": ..., "object": "model"}`, also the cloud routes. Return, per route:

```json
{"id": "deepseek-v41-flash", "object": "model", "owned_by": "deepseek", "name": "DeepSeek-V4.1-Flash",
 "context_window": 1048576, "max_output_tokens": 393216,
 "input_modalities": ["text", "image"], "output_modalities": ["text"],
 "effort": {"supported_levels": ["low", "high", "max"], "default_level": "high"},
 "api_capabilities": {"anthropic_messages": {"system_prompt_update": "in-history"}}}
```

* New nullable columns on `model_routes`: `display_name`, `owned_by`, `input_modalities`, `output_modalities`, `effort_levels`, `effort_default`, `system_prompt_update`.
* `context_window` comes from the existing `context_tokens`; `max_output_tokens` from the existing `max_output_tokens` column (stored today, not exposed). For `deepseek-v41-flash` set `max_output_tokens` to 393216 (it is NULL now).
* A NULL field is left out of the JSON (all of these are optional in the cloud schema), so routes without values keep today's output.
* `effort.supported_levels` must not contain `none` (cloud rule); `default_level` must be one of the levels.
* The harness builds its effort picker from `effort.supported_levels`: this is the field that is missing.

## B. Request: turn cloud parameters into vLLM parameters

Merge into the body's `chat_template_kwargs` before forwarding; never overwrite a key the client already set there:

| client sends | set in `chat_template_kwargs` |
|---|---|
| `thinking: {"type": "disabled"}` | `thinking: false` |
| `thinking: {"type": "enabled"}` | `thinking: true` |
| effort `none` | `thinking: false` |
| effort `minimal`, `low` | `reasoning_effort: "low"` |
| effort `medium`, `high`, `xhigh` | `reasoning_effort: "high"` |
| effort `max`, `ultra` | `reasoning_effort: "max"` |
| effort integer | `reasoning_effort: clamp(n, 1, 100)` |

Where the effort comes from, in this order: `reasoning_effort`, then `reasoning.effort`, then `output_config.effort`. After the rewrite delete `thinking`, `reasoning_effort`, `reasoning`, `output_config` from the forwarded body. (vLLM ignores them as top-level fields, so today effort has no effect.)
Measured on the shim: same prompt, `reasoning_tokens` differ between low / default / max, and are 0 with thinking disabled or effort `none`.

## C. Assistant messages sent back: keep exactly one reasoning field

The harness appends the whole assistant message (which carries both `reasoning` and `reasoning_content`, as returned by D) to the next request. vLLM treats `reasoning_content` as an alias of `reasoning` on input, so sending both returns HTTP 400 `duplicate field 'reasoning'`. For every message in `messages`: take `reasoning_content` if present and not null, else `reasoning`, remove both keys, and set a single `reasoning_content` when the value is not null. Needed for tool calling with thinking on (cloud requires `reasoning_content` of earlier turns to be passed back when `tools` is present).

## D. Response: add the cloud fields, keep vLLM's

* Non-stream: for each choice, if `message.reasoning` is not null, add `message.reasoning_content` with the same value (keep `reasoning`; both are returned).
* Stream (SSE): same for every `choices[i].delta`, chunk by chunk; leave `data: [DONE]` untouched and flush per chunk.
* `usage`: add `prompt_cache_hit_tokens` = `prompt_tokens_details.cached_tokens` (0 if null) and `prompt_cache_miss_tokens` = `prompt_tokens - hit`. `completion_tokens_details.reasoning_tokens` is already returned by vLLM.

## E. Model name alias (not verified as the owner's cause)

vLLM accepts only the name `deepseek-v41-flash`. Routes `deepseek-flash`, `deepseek-v4-pro`, `deepseek-flash-steve` point to DeepSeek cloud (backend 4) today. If the harness hard-codes `deepseek-flash`, decide which route is the local one and set its `provider_model_name` to `deepseek-v41-flash`. In the router ledger for 2026-11-01 02:48-02:58 the harness traffic was only `deepseek-flash` / `deepseek-v4-pro` on backend 4, none on `deepseek-v41-flash`.

## F. Acceptance tests (run after the change, no shim)

1. `curl http://<router>/v1/models` shows `effort.supported_levels` for `deepseek-v41-flash`.
2. Same question with `reasoning_effort` `low`, `max`, and with `thinking: {"type": "disabled"}`: `usage.completion_tokens_details.reasoning_tokens` differs, and is 0 when thinking is disabled.
3. Streaming response has `delta.reasoning_content`.
4. Tool round trip: send the assistant message back unchanged (both reasoning fields present, `tool_calls` set) followed by the `tool` message: HTTP 200 and a normal answer.
5. A route to DeepSeek cloud is unchanged.

Known limits of the shim test: not run against the owner's harness (its requests never reached the shim), not load-tested.
