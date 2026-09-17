# CODEX audit — Portal live static mode + guided route wizard gate PASS — 2026-09-17

## Verdict

PASS for this round. The repo now has a sane Portal editing path and the model-route wizard has a clearer, testable flow.

This does not mean the Portal is final SOTA design. It means the previous blocking problems for refresh/rebuild confusion and route-from-provider regression are fixed and covered by tests.

## What changed

1. Portal front-end source is documented as one file: `static/index.html`.
2. Rust still embeds `static/index.html` into the production binary with `include_str!`, so the production Docker image remains simple and self-contained.
3. Docker Compose can now run live Portal design mode by setting:

```text
PORTAL_STATIC_FILE=/app/static/index.html
```

When set, Rust reads `/app/static/index.html` on each Portal request. A UI designer can edit `static/index.html` and press F5 without rebuilding the Rust binary.

4. `docker-compose.yml` mounts `./static:/app/static:ro` for that live mode.
5. `static/index.html` now has a more explicit Add model wizard:
   - step 1: Connect
   - step 2: Choose model
   - step 3: Test & save
6. Fixed the Route-from-provider modal bug: selecting Route from a provider connection now opens Add model with the provider/base URL path instead of hitting the stale `psel` variable error.
7. README, INSTALL, and PROVIDERS now describe the current one-flow setup:
   - provider catalog comes from `.env`
   - admin creates model routes from Models & Routes
   - key can be pasted in the wizard or resolved from env
   - route should be tested before save enabled

## Runtime proof

Docker container was recreated and served the updated Portal over HTTPS.

Live static mode was verified with a temporary sentinel edit in `static/index.html`; `curl https://127.0.0.1:18443/` saw the sentinel without rebuilding the Docker image. The sentinel was then removed.

Important operational note: because the container runs as non-root, host-side `static/` must be readable by the container. On this machine that required:

```bash
chmod 755 static
chmod 644 static/index.html
```

## Gates run

- `cargo fmt`
- `cargo test --workspace`
- `docker build -t thusinh1969/brighto_airouter:v1 .`
- `docker compose up -d --force-recreate router`
- `swarm/scripts/portal_empty_state_audit.sh`
- `swarm/scripts/portal_logic_acceptance.sh`
- `swarm/scripts/portal_visual_audit.sh`
- `swarm/scripts/portal_polish_audit.sh`
- `swarm/scripts/portal_full_page_audit.sh`

All passed in the latest local gate run.

## Remaining product-design bar

The next design pass should focus on visual density, model-name overflow, dashboard information hierarchy, and fewer low-value timing fields. The current implementation is now easier to iterate because UI-only work can happen in `static/index.html` with browser refresh.
