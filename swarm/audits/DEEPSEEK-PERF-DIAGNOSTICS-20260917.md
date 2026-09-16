# DeepSeek — performance diagnostics (overhead + prompt buckets) — 2026-09-17

Verdict: /admin/summary now exposes router-overhead p95 and latency grouped by prompt-size
bucket; Portal Dashboard has a dedicated "Performance diagnostics" section. Self-audit green.

## Done
- /admin/summary: totals.p95_router_overhead_ms (percentile_cont on router_overhead_ms), and
  by_bucket[] = { bucket ('<2k'|'2k-32k'|'32k-128k'|'128k+'), requests, p95_ttfb_ms,
  p95_total_ms, p95_router_overhead_ms } grouped by input-token bucket.
- Portal Dashboard: "Performance diagnostics" panel shows Router overhead p95 (friendly
  duration) + a "Latency by prompt size" table (bucket/requests/p95 first byte/p95 total/p95
  router). No global mixed-prompt-size latency card anywhere.
- JS fmtDur() formatter (123 ms / 1.8 s / 3m 48s) reused for latency cells.

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests.
- portal_smoke: PASS (19 checks; summary now asserts by_bucket + p95_router_overhead_ms).
- real_provider_smoke: PASS (8 checks).
- node --check portal JS: PASS.

## Remaining
- Provider health table (429/5xx/timeout per provider) on dashboard.
- HTTPS/TLS (Rustls) + start.sh helpers + healthcheck under HTTPS.
- Provider template presets (verified vs experimental) + Anthropic model-list.
