# CODEX AUDIT — Real-provider cost guard + local llama.cpp results

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Hard rule for paid provider tests

Do not run 1M, 5M, or 10M token tests against paid cloud providers.

Real provider tests are capped at:

- 1k prompt-token class
- 50k prompt-token class
- 200k prompt-token class
- tiny output only, normally `max_tokens: 8` or similar
- sequential ramp, stop immediately on provider error, context error, body limit, 429, or abnormal latency

Large stress tests such as 500k–1M+ are for mock/local providers only. They prove router pass-through behavior, memory profile, concurrency behavior, timeout behavior, and ledger correctness without burning paid API budget.

## Rate-limit rule for DeepSeek

For DeepSeek paid smoke:

1. Use one request at a time.
2. Use low output token cap.
3. Start 1k, then 50k, then 200k only if the previous tier succeeds.
4. Do not retry automatically on 429.
5. Record provider status/body-limit/context-limit clearly.
6. Never print the full API key in terminal, audit, logs, or committed artifacts.

## Secret handling blocker found live

A process was observed with a real provider key embedded in the shell command line. That is unacceptable because command-line arguments can be visible via `ps`.

Required fix for test scripts:

- Do not place provider keys inside shell command strings.
- Prefer reading the key from a root-owned temp file outside git or from stdin.
- Redact `sk-*` style tokens in all logs before printing.
- Never commit real keys or run artifacts containing real keys.

Codex stopped the observed real-provider smoke process to avoid cost and key exposure.

## Local llama.cpp router smoke results

Local endpoint supplied by user:

- llama.cpp/OpenAI-compatible URL used by router: `http://127.0.0.1:8088/v1`
- model: `qwen3.8-flash-next`
- key: empty / no-auth intended

Direct `/v1/models` returned the model successfully.

Current committed runtime still required a fake provider key, so Codex used a dummy local file key for this test. llama.cpp ignored the dummy Bearer header and the route succeeded. This proves the data path but does not satisfy the final no-auth UX requirement.

Measured through BrighTO-Router, non-streaming, tiny output cap:

| Target prompt size | HTTP payload | Provider-reported prompt tokens | Provider-reported total tokens | Status | End-to-end latency | Router overhead header |
|---:|---:|---:|---:|---:|---:|---:|
| 1k | 6,170 bytes | 1,058 | 1,066 | 200 | 1.792s | 0ms |
| 50k | 300,170 bytes | 50,058 | 50,066 | 200 | 40.911s | 2ms |
| 200k | 1,200,170 bytes | 200,058 | 200,066 | 200 | 228.453s | 3ms |

Interpretation:

- Router passed 1k/50k/200k prompt classes successfully to local llama.cpp.
- The long latency is dominated by local model prefill/context handling, not router overhead.
- Header `x-router-overhead-ms` stayed 0–3ms in this sequential local run.
- This is functional pass-through evidence, not a concurrency benchmark.

Artifact kept locally, not committed because `swarm/out` is ignored:

- `swarm/out/local-llama/20260917-042015/summary.json`

## Still required from DeepSeek

1. Finish true no-auth provider support in a clean product contract.
   - Current dirty code direction is partially right: skip auth header when key is missing.
   - Still needs an explicit product/API concept so admins can select “Local server, no API key”.
   - `create_backend` must not force fake `api_key_ref` for no-auth providers.
   - Portal must not show no-auth as an error state.

2. Do not rely on fake env refs like `env:DUMMY_EMPTY` as the final UX.

3. Add a real no-auth acceptance test:
   - create backend with no key
   - load models
   - create route
   - call router
   - assert provider received no `Authorization` and no `x-api-key`

4. Keep stress benchmark separation clear in docs:
   - mock/local for 500k–1M+ stress
   - paid providers only for small functional smoke at 1k/50k/200k

