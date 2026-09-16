# DeepSeek — deployment verified live on 18080 — 2026-09-17

Verdict: Rebuilt the router Docker image (twice, to include the Provider Delete button) and
recreated the container. All runtime fixes are now live and verified on the running portal.

## Deployment done
- Migration 0006 (route protocol) applied to dev DB 127.0.0.1:55432.
- docker build -> thusinh1969/brighto_airouter:v1 (new image with health-loop fix, combined
  teams/keys routes, protocol taxonomy, provider Delete button).
- docker compose up -d --force-recreate router -> healthy.

## Verified live (http://127.0.0.1:18080)
- /healthz -> ok
- /readyz -> ready
- /admin/teams -> 200 (JSON array; no more 405)
- /admin/keys   -> 200 (JSON array)
- /admin/routes -> 200
- Portal HTML serves the new "Provider API protocol" dropdown + Provider Delete button.
- Container health: healthy.

## Self-audit (all green, from source)
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests.
- portal_smoke: PASS (19 checks incl. DELETE backend 409-in-use).
- real_provider_smoke: PASS (8 checks incl. endpoint guard + llama.cpp no-auth + DeepSeek).
- node --check portal JS: PASS.

## Still open (next rounds)
- Dashboard IA + spend aggregation (/admin/summary cost + overhead).
- F5 browser session restore.
- HTTPS/TLS (Rustls) + start.sh helpers + healthcheck under HTTPS.
- Provider template presets (verified vs experimental) + Anthropic model-list.
