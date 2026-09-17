# CODEX AUDIT — Portal polish gate now verifies tok/s with real request log

Date: 2026-09-17 18:00 +07
Live target: `https://127.0.0.1:18443`
Docker image rebuilt/recreated: `thusinh1969/brighto_airouter:v1`

## Verdict

The specific `tok/s` logging gap is closed for successful token responses, including ultra-fast mock/local calls that measure as `0 ms` at millisecond resolution.

## Root cause

Admin usage rows calculated throughput with `tokens_per_second(tokens, total_ms)`. For very fast local/mock calls, `total_ms` can be `0`, so the API returned `total_tokens_per_second=null` and the Portal displayed `—` in the `TOK/S` column even though `output_tokens > 0`.

## Fix

- `src/admin/mod.rs`: throughput now clamps positive-token rows with `ms <= 0` to a 1ms denominator, so they render a useful observed tok/s value instead of `null`.
- `static/index.html`: `TOK/S` display uses compact K/M/B formatting via `fmt(...)`.
- `swarm/scripts/portal_polish_audit.mjs`: now seeds a mock route/key, sends a real `/v1/chat/completions` request, verifies the request log row has `total_tokens_per_second`, and verifies the UI renders the compact value.
- `swarm/scripts/portal_polish_audit.sh`: cleans up only `pw-polish-*` test data before and after the audit, including ledger rows, so the gate does not pollute local PostgreSQL.

## Verification commands run

```bash
cargo test --workspace
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_visual_audit.sh
bash swarm/scripts/portal_polish_audit.sh
```

Results:

- Rust tests: PASS, 64/64.
- Docker build/recreate: PASS.
- Logic acceptance: PASS.
- Visual responsive audit: PASS.
- Polish audit: PASS.

Latest polish evidence:

- smoke request HTTP status: `200`
- usage row: `input_tokens=25`, `output_tokens=64`, `total_tokens=89`
- API `total_tokens_per_second=89000`
- Portal rendered compact value: `89K`

Cleanup verification after polish audit:

```text
pw_polish_routes=0
pw_polish_usage=0
pw_polish_backends=0
pw_polish_keys=0
```

## Remaining quality scope

Known automated gates are green. This still does not prove the entire Portal is OpenRouter/Hermes-level. A broader screenshot audit of every page is still required before claiming the full objective is complete.
