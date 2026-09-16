# DeepSeek — protocol taxonomy + runtime blockers fixed — 2026-09-17

Verdict: Landed route-level provider protocol + endpoint guard (CODEX protocol taxonomy) and
fixed the two runtime blockers CODEX flagged (health-loop hang + duplicate teams/keys routes).
Full self-audit green. Docker image rebuild in progress for the 18080 portal.

## Done

### 1. Route-level provider protocol (CODEX provider-protocol taxonomy)
- contract::ProviderProtocol: openai_chat | openai_completions | openai_embeddings |
  anthropic_messages | local_openai_chat | custom_openai_chat, with parse/as_str/
  incoming_path/label.
- ModelRoute now carries protocol; migration 0006 adds model_routes.protocol
  (default 'openai_chat') + backfills anthropic-format routes to 'anthropic_messages'.
- Config loader reads+normalizes protocol; admin CRUD (create/PATCH) persists it and
  GET /admin/routes returns it. Backward-compat default: anthropic auth -> anthropic_messages,
  else openai_chat.
- Protocol endpoint guard in proxy hot path: /v1/chat/completions accepts openai_chat/
  local_openai_chat/custom_openai_chat; /v1/completions -> openai_completions;
  /v1/embeddings -> openai_embeddings; /v1/messages -> anthropic_messages. Wrong endpoint
  returns 400 "model 'X' is a <Label> route (call <path>)" — no forward of wrong shape.
- Portal route wizard: new "Provider API protocol" dropdown (6 presets) that sets auth default
  (anthropic->x-api-key, local->none, else bearer) on create; Load models maps protocol ->
  openai/anthropic for preview-models; Save sends protocol; routes table shows a Protocol column.

### 2. Runtime blockers (CODEX portal-spinning audit)
- Health loop: health_check_all() no longer holds a DashMap entry guard across .await — it
  collects enabled (id, base_url) targets first, drops guards, then awaits network I/O. Removes
  the Tokio-worker block that made /healthz hang when any backend was enabled.
- Duplicate axum routes: /teams and /keys are now single combined registrations
  (.route("/teams", get(list_teams).post(create_team)), same for /keys) — no more 405 for
  GET /admin/teams or GET /admin/keys.

## Self-audit (all green)
- cargo fmt + clippy -D warnings: PASS
- cargo build --release --locked: PASS
- 60 lib + 4 integration tests: PASS (incl. 2 new ProviderProtocol tests)
- portal_smoke: PASS (17 checks; GET /admin/teams + GET /admin/keys 200, DELETE backend,
  enriched usage)
- real_provider_smoke: PASS (8 checks; llama.cpp no-auth chat, DeepSeek route credential,
  endpoint guard "chat route rejects embeddings" -> 400)
- node --check portal JS: PASS

## Remaining (next rounds)
- Dashboard IA full redesign + spend aggregation (ledger cost) + /admin/summary cost/overhead.
- F5 browser session restore (admin/user TTL).
- HTTPS/TLS (Rustls) + start.sh TLS helpers + healthcheck under HTTPS.
- Anthropic Messages model-list (manual entry fallback today).
- Provider template presets (verified vs experimental) + provider edit identity UI.
