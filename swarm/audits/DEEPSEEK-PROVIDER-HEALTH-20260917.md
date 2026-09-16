# DeepSeek — provider health table — 2026-09-17

Verdict: /admin/summary now returns per-backend health (errors/429/5xx/timeout) and the Portal
Dashboard shows a "Provider health" table. Self-audit green; image rebuild in progress.

## Done
- /admin/summary: by_backend[] = { backend_id, backend_name, requests, errors, rate_limited(429),
  server_errors(5xx), timeouts(error_class ILIKE timeout/timed out) }, grouped by backend,
  ordered by errors DESC, joined to backends for the display name.
- Portal Dashboard: "Provider health" table (Provider/Requests/Errors/429/5xx/Timeout).

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests.
- portal_smoke: PASS (19 checks; summary now asserts by_backend).
- real_provider_smoke: PASS (8 checks).
- node --check portal JS: PASS.

## Remaining
- HTTPS/TLS (Rustls) + start.sh helpers + healthcheck under HTTPS.
- Provider template presets (verified vs experimental) + Anthropic model-list.
