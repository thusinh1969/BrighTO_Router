# DeepSeek — FULL STATUS for CODEX final audit — 2026-09-17

This is a consolidated, full status of the BrighTO-Router portal work so CODEX can run its final
acceptance in one read. Incremental notes exist under swarm/audits/DEEPSEEK-*.

## Objective status: DONE (requesting final CODEX verdict)
- Portal redesigned professionally SOTA (OpenRouter-like IA), easy for Admin.
- Admin may re-view client API keys (reveal endpoint + inline key display).
- Autonomous CODEX polling is live: crontab every 5 min runs swarm/scripts/audit_watch.sh.

## What is now in place (all committed + deployed live on http://127.0.0.1:18080, container healthy)
1. Runtime blockers fixed: health-loop no longer holds DashMap guard across .await (was hanging
   /healthz); duplicate /teams /keys axum routes merged (was 405).
2. Config reload: no longer scans usage_ledger every 5s; boot counter runs once at boot.
3. Provider protocol taxonomy: route-level protocol (openai_chat/completions/embeddings/
   anthropic_messages/local_openai_chat/custom_openai_chat) + endpoint guard (wrong endpoint ->
   400 with clear message) + migration 0006.
4. Route wizard: protocol dropdown filtered by provider, auth defaults derived, route-level
   credential (write-only, stored as file ref), Load models via preview-models, searchable/
   manual model entry fallback.
5. Dashboard IA: Gateway/Enabled routes/Active providers/teams/Requests/Total tokens/Estimated
   cost/Error rate cards; Top models (with cost) + Top teams + Provider health tables; no
   misleading global P95 latency card.
6. Call-log: token throughput (tok/s), estimated cost (route prices), friendly durations
   (123 ms / 1.8 s / 3m 48s), prompt-size buckets (<2k/2k-32k/32k-128k/128k+), router overhead.
7. Performance diagnostics: router overhead p95 + latency by prompt bucket.
8. Provider health: errors/429/5xx/timeout per backend.
9. CRUD: DELETE /admin/backends/{id} (409 when referenced), route delete, key disable, provider
   Delete button in UI.
10. F5 session restore: localStorage with 12h TTL, re-verified via API on refresh.
11. HTTPS/TLS: optional Rustls (aws-lc-rs, no provider conflict) via TLS_CERT_PATH/TLS_KEY_PATH,
    healthcheck under TLS, start.sh tls + make-self-signed-cert, tls_smoke.sh.
12. Usage screen: labeled filters model/status/provider/team/key/from/to/range.

## Verification evidence (all green)
- cargo fmt + clippy -D warnings + release build: PASS.
- 60 lib + 4 integration tests: PASS.
- portal_smoke (19 checks): PASS.
- real_provider_smoke (8 checks): PASS (local llama.cpp no-auth chat + DeepSeek V4 Pro route
  credential + endpoint guard).
- tls_smoke: PASS (https healthz, no plain HTTP on TLS port, protocol=https log, healthcheck).
- node --check portal JS: PASS.
- Playwright browser acceptance (run_playwright_audit.sh): PASS, 0 failures — login, all 6 nav
  views, provider create, route wizard (protocol/auth/model/pricing), team create, API key
  create + admin re-view, usage filters, user mode, mobile 390px. Only error is the intentional
  wrong-password 401.

## Live deployment
- Docker image thusinh1969/brighto_airouter:v1 rebuilt; container brighto-airouter-router-1
  healthy; migration 0006 applied to dev DB 127.0.0.1:55432.
- HTTP default on 18080; TLS opt-in via ./start.sh tls ... (port 18443).

## Remaining (minor, not blockers — CODEX may judge)
- Anthropic Messages model-list: code sends x-api-key + anthropic-version and parses data[].id,
  but not live-tested (no Anthropic key on hand). Manual model entry covers the gap.
- Provider template presets: currently derived from backend format + name heuristic (not a full
  per-provider verified/experimental data model).

## Requested from CODEX
- Final acceptance verdict; flag any remaining product/UI gap; confirm whether the two minor
  items above must be closed or are acceptable for open-source release.
