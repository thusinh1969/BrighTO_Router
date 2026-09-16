# CODEX plan — real DeepSeek V4.0 Pro smoke after Portal gate — 2026-09-17

User provided a real DeepSeek provider API key for later testing. Do **not** write that secret into Git, audits, README, screenshots, shell output, benchmark artifacts, or final messages.

## When this plan may run

Run only after the Portal/API product gate is green:

```text
cargo check/fmt/clippy/test/smoke green
Playwright admin + user flows green
Create Model Route wizard accepted
Provider key write path accepted
```

Do not spend real provider tokens while the Portal still has broken setup flow.

## Provider/model setup

Target provider:

```text
DeepSeek
```

Target public route name:

```text
deepseek-v4-pro
```

Target provider model:

```text
DeepSeek V4.0 Pro
```

If the provider API returns a different exact model id, record the actual model id in local result artifacts and use that id as `provider_model_name`. Do not invent model names.

## Key handling

Use the key only as a write-only provider credential:

- preferred: Portal wizard/API saves it through `PUT /admin/backends/{id}/key` or equivalent;
- fallback only if the endpoint is not ready: set local env/file key reference manually for testing;
- never echo the key;
- never commit `.env` containing the key;
- never include the key in screenshots.

Provider plaintext key must never be returned by API list/detail endpoints.

## Payload sizes

Run three small real-provider pass-through tests:

```text
1k input tokens
50k input tokens
200k input tokens
```

Keep output small to control cost:

```text
max_tokens: 16 or 32
stream: false for first pass
```

If 200k exceeds provider context/window or account limits, record the exact provider error and do not retry blindly.

## Measurements to record

For each payload:

```text
payload name
approx input tokens
request body bytes
HTTP status
provider error class if any
time to first byte if streaming is used
total latency
router overhead if direct-vs-router test is possible
input/output tokens from provider usage if returned
usage ledger row correctness
router RSS before/after
```

Cost:

- record cost only if route price is configured or provider response clearly exposes billable usage;
- otherwise write `cost: unknown`;
- never fake spend.

## Safety limits

- Run sequentially first: concurrency 1.
- Do not run 50/200 concurrency against real DeepSeek unless user explicitly asks after reviewing cost and limits.
- Set a request timeout long enough for 200k but finite.
- Save sanitized artifacts under:

```text
swarm/out/real-deepseek/<timestamp>/
```

Do not commit those artifacts unless the user asks.

## Acceptance

A real-provider smoke passes only if:

- route can be configured through the Portal/API;
- all three payloads either succeed or fail with documented provider/account limits;
- no provider key leaks;
- usage ledger rows match the request outcomes;
- memory remains stable enough for the payload size.
