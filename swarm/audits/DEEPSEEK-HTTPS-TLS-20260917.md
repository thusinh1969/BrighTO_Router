# DeepSeek — HTTPS/TLS runtime + helpers — 2026-09-17

Verdict: Implemented the optional TLS runtime (Rustls, aws-lc-rs — no crypto-provider conflict),
healthcheck under HTTPS, a tls_smoke script, and start.sh helpers. HTTP remains the default.

## Done
- Cargo.toml: axum-server gains tls-rustls (rustls/aws-lc-rs, matching reqwest+sqlx).
- main.rs: reads TLS_CERT_PATH + TLS_KEY_PATH. Both unset -> HTTP (today). Both set -> HTTPS via
  axum_server::tls_rustls::bind_rustls. Only one set -> fail fast. Logs protocol=http|https.
  Graceful shutdown now uses axum_server::Handle.
- healthcheck subcommand: uses https:// when TLS envs set, and accepts the local self-signed cert
  (danger_accept_invalid_certs only for healthcheck, not upstream provider calls).
- scripts/tls_smoke.sh: generates a throwaway self-signed cert, boots HTTPS, asserts https healthz
  ok, plain HTTP on the TLS port fails, protocol=https logged, healthcheck subcommand succeeds.
- start.sh: make-self-signed-cert [HOST] and tls --cert --key --host --port (copies to ssl/,
  chmod 600, sets .env LISTEN_ADDR/BASE_URL/TLS_*, recreates router). docker-compose mounts
  ./ssl:/certs:ro.

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests.
- portal_smoke + real_provider_smoke: PASS (HTTP default path unchanged).
- tls_smoke: PASS (https healthz, no plain HTTP, protocol log, healthcheck under TLS).
- bash -n start.sh: PASS; docker compose config: PASS.

## Note
- Dev portal stays HTTP on 18080; TLS is opt-in via ./start.sh tls ... (per CODEX: do not enable
  until needed). TLS port will be 18443 when enabled.

## Remaining
- Provider template presets (verified vs experimental) + Anthropic model-list.
