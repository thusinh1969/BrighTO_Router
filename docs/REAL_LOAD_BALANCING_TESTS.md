# Real load-balancing smoke test

This local-only test proves BrighTO-Router can balance one client-facing Model Group across a live cloud LLM endpoint and a live local llama.cpp endpoint.

It is intentionally small. It spends only a few tiny DeepSeek calls and uses `max_tokens: 4`.

## What it creates

The script creates or updates these local dev records through the Admin API:

| Record | Name | Purpose |
|---|---|---|
| Backend | `live-lb-deepseek-backend` | DeepSeek OpenAI-compatible endpoint |
| Backend | `live-lb-qwen38-backend` | local llama.cpp endpoint |
| Model route | `live-lb-deepseek-v4-pro` | direct DeepSeek V4 Pro route |
| Model route | `live-lb-qwen38` | direct local Qwen3.8 route |
| Model Group | `live-lb-rr` | round-robin group across both routes |
| Model Group | `live-lb-weighted` | weighted round-robin group, DeepSeek weight 3 and local Qwen weight 1 |
| Team/API key | `Live LB Test` | temporary client key used by the script |

These records are left in the local database so they are visible in the Portal after the run. Do not use this script against a production database unless you intentionally want those test records.

## Requirements

- Router is running locally, usually `https://127.0.0.1:18443`.
- `.env` contains `ADMIN_MASTER_KEY`.
- `.env` contains `DEEPSEEK_API_KEY` or `DEEPSEEK_KEY`.
- local llama.cpp is reachable at `http://127.0.0.1:8088/v1` and exposes `qwen3.8-flash-next`.

The script never prints provider keys or generated client API keys.

## Run

```bash
python3 scripts/live_lb_real_test.py
```

Optional flags:

```bash
python3 scripts/live_lb_real_test.py \
  --router https://127.0.0.1:18443 \
  --local-base http://127.0.0.1:8088/v1 \
  --deepseek-base https://api.deepseek.com \
  --deepseek-model deepseek-v4-pro \
  --local-model qwen3.8-flash-next
```

## Expected result

A passing run prints:

```text
PASS local preview includes qwen3.8-flash-next
PASS DeepSeek preview includes deepseek-v4-pro
PASS source routes saved; DeepSeek credential stored as key reference
PASS Model Groups saved: round_robin and weighted_round_robin 3:1
PASS call source DeepSeek: live-lb-deepseek-v4-pro
PASS call source Local Qwen: live-lb-qwen38
Round-robin usage counts: DeepSeek=2, LocalQwen=2, statuses=[200]
Weighted usage counts: DeepSeek=3, LocalQwen=1, statuses=[200]
RESULT PASS real DeepSeek+Qwen round-robin and weighted Model Group test
```

The important proof is the usage ledger distribution, not the response text:

- `live-lb-rr` must record 4 successful requests split 2/2 across DeepSeek and local Qwen.
- `live-lb-weighted` must record 4 successful requests split 3/1 across DeepSeek and local Qwen.

If a provider is down or rate-limited, the script fails with the exact failing API step and sanitized status text.

## Fail-safe and recovery timing

Model Group fail-safe is circuit-breaker based. After repeated pre-response failures, the backend is removed from selection and a half-open probe is allowed after `BACKEND_CIRCUIT_OPEN_SECONDS` seconds. The default is `30`. Set it in `.env` and restart the router to change the recovery probe interval.
