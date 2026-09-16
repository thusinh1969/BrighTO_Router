# CODEX AUDIT — Real provider smoke matrix correction

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Provider smoke tests must be tightly scoped and must not accidentally include unrelated local endpoints from `.model`.

The user clarified:

- Local test target is only the currently running llama.cpp endpoint:
  - base URL: `http://127.0.0.1:8088/v1`
  - provider model: `qwen3.8-flash-next`
  - auth: no API key
- Do not include other local `10.x` Gemma/Qwen/vLLM entries from `.model` in this test matrix.
- Gemini is excluded for now.
- Qwen must not use token-plan endpoint.
- Kimi K3 must use the normal Kimi/Moonshot API endpoint.

## Allowed test targets

Use `.model` only as secret/model metadata source for these cloud providers:

| Provider | Include? | Protocol/auth expectation | Notes |
|---|---:|---|---|
| OpenAI | Yes | OpenAI Chat Completions compatible, Bearer key | Functional smoke only. |
| Anthropic | Yes | Anthropic Messages, x-api-key | Functional smoke only. |
| DeepSeek | Yes | OpenAI Chat Completions compatible, Bearer key | Use only small 1k/50k/200k max if doing paid ramp; normal smoke can be one tiny call. |
| Kimi K3 / Moonshot | Yes | OpenAI Chat Completions compatible, Bearer key | Use `https://api.moonshot.ai/v1`, not token-plan. |
| Qwen | Yes | OpenAI Chat Completions compatible, Bearer key | Exclude any `token-plan` base URL. Use normal workspace/base URL from `.model`. |
| GLM / Z.AI | Yes | OpenAI-compatible if endpoint supports it | Use `.model` entry `glm-5.2` / Z.AI-compatible base. |
| Agnes | Yes | Custom OpenAI-compatible | Use `.model` Agnes API base/key. |
| Gemini | No | Excluded | Do not test now. |
| Local llama.cpp | Yes | Local OpenAI-compatible Chat, no-auth | Only `127.0.0.1:8088/v1`, model `qwen3.8-flash-next`. |
| Local 10.x Gemma/Qwen/vLLM | No | Excluded | Do not test. |
| Meta Muse | No until clarified | `.model` entry appears malformed/unclear | Do not spend cloud calls on unclear config. |
| xAI/Grok | No unless user explicitly asks | Not in current requested list | Skip. |

## Functional-smoke scope

Do not benchmark paid providers.

For each allowed cloud provider:

1. Create a route through Admin API using route-level credential.
2. If model list endpoint is known to work, call model preview/list once.
3. Send one tiny request with small output cap, for example `max_tokens: 8`.
4. Confirm status is 2xx and response shape is parseable.
5. Confirm usage ledger records provider/model/team/key.
6. Confirm no provider key appears in logs, audit files, command line, or committed artifacts.

If provider returns unsupported model-list, do not fail the whole provider. Mark:

- model list: not supported / manual model required;
- chat/messages call: pass/fail separately.

## Cost/rate-limit guard

Cloud providers:

- one request at a time;
- no concurrency;
- no retries on 429;
- no 500k/1M/10M paid calls;
- use `max_tokens <= 8` for functional smoke;
- only DeepSeek may get 1k/50k/200k ramp if explicitly needed, and even then stop on first rate-limit/context/error.

Mock/local benchmark remains the place for 500k–1M stress.

## Secret handling

`.model` is a secret file and must stay untracked.

Test scripts must:

- parse `.model` without printing API keys;
- not put keys in command-line arguments;
- not store plaintext keys in `swarm/out`;
- redact `sk-*`, bearer tokens, and long token-like values from stdout/stderr;
- write only provider name, public model, status, latency, token counts, and sanitized error class.

## Route wizard acceptance against these providers

DeepSeek must make Admin route creation support these practical cases:

1. OpenAI Chat Completions route with Bearer key.
2. Anthropic Messages route with x-api-key.
3. Kimi K3 route through normal Moonshot API.
4. Qwen route through non-token-plan compatible endpoint.
5. GLM/Z.AI route through compatible endpoint.
6. Agnes custom OpenAI-compatible route.
7. Local llama.cpp no-auth route.

For each one, route wizard must show protocol clearly, because `openai` vs `anthropic` is too coarse.

## Do not include

Do not include these in current provider smoke:

- Gemini;
- local `10.x` Gemma/Qwen/vLLM entries;
- Qwen token-plan endpoint;
- Meta Muse until `.model` entry is corrected;
- huge cloud stress tests.
