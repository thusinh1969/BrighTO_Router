# CODEX AUDIT — RED BUILD BLOCKER: route `protocol` patch is incomplete

Date: 2026-09-17 05:25 ICT  
Role: Codex auditor/mentor only — no product code changed.

## Verdict

**BLOCKER — current tree does not compile.**

The route-level `protocol` work is the right architectural direction, but the patch is partial. `ModelRoute` and `RouteResponse` now require `protocol`, while several constructors and admin responses still do not set it. Until this is fixed, Docker build / CI / local `cargo check --locked --all-targets` fail.

## Evidence from current working tree

Current dirty files:

```text
 M src/admin/mod.rs
 M src/config/mod.rs
 M src/contract.rs
?? migrations/0006_route_protocol.sql
```

Failing command:

```bash
cargo check --locked --all-targets
```

Compiler failures:

```text
error[E0063]: missing field `protocol` in initializer of `RouteResponse`
   --> src/admin/mod.rs:634:18

error[E0063]: missing field `protocol` in initializer of `RouteResponse`
    --> src/admin/mod.rs:1197:5

error[E0063]: missing field `protocol` in initializer of `contract::ModelRoute`
   --> src/handlers.rs:738:21

error[E0063]: missing field `protocol` in initializer of `contract::ModelRoute`
   --> src/route/mod.rs:513:9

error[E0063]: missing field `protocol` in initializer of `contract::ModelRoute`
   --> src/route/mod.rs:594:9
```

## Root cause

`protocol` was added to the shared runtime contract, but the patch did not update every boundary that creates, reads, writes, serializes, or validates a route.

There is also a semantic bug in `preview_models`: it parses `payload.protocol` with `normalize_backend_format()`. That function is for old backend format values (`openai`, `anthropic`), not route protocol values (`openai_chat`, `local_openai_chat`, `anthropic_messages`, etc.). This will break the route wizard flow even after the compile errors are fixed.

## One-pass fix DeepSeek should do

1. **Compile fix: add `protocol` everywhere required.**
   - `src/admin/mod.rs:list_routes_from_pool()` must select `protocol` and fill `RouteResponse.protocol`.
   - The route response builder around `src/admin/mod.rs:1197` must fill `protocol`.
   - Test/helper `ModelRoute` initializers in `src/handlers.rs` and `src/route/mod.rs` must set `protocol: "openai_chat".to_string()` or equivalent.

2. **Do not reuse backend format validation for protocol.**
   - Add a dedicated function such as `normalize_provider_protocol(raw: Option<&str>) -> Result<String, ApiError>`.
   - Accept exactly these values for now:
     - `openai_chat`
     - `openai_completions`
     - `openai_embeddings`
     - `anthropic_messages`
     - `local_openai_chat`
     - `custom_openai_chat`
   - Default missing/empty route protocol to `openai_chat` for backward compatibility.
   - Reject unknown values with a clear admin error. Do not silently map arbitrary strings to OpenAI except at the DB loader compatibility layer if needed for old rows.

3. **Persist and update protocol through admin CRUD.**
   - `UpsertRoute` already has `protocol: Option<String>`; validate it and write it to `model_routes.protocol` on create/update.
   - `PATCH/PUT` route update must keep existing protocol if omitted, and update it if provided.
   - `GET /admin/routes` must return protocol so the UI can show/edit the existing value.
   - Existing rows should be covered by migration default `'openai_chat'`.

4. **Fix `preview_models` auth/header logic.**
   - `openai_chat`, `openai_completions`, `openai_embeddings`, `local_openai_chat`, `custom_openai_chat` all use `/v1/models` and Bearer auth unless `auth_mode = none`.
   - `anthropic_messages` should use Anthropic headers (`x-api-key`, `anthropic-version`). If Anthropic model-list support is weak or unavailable, return a clean admin-facing error and allow manual model name entry.
   - Local llama.cpp no-auth must work with `auth_mode = none` and base URL like `http://127.0.0.1:8088/v1`.

5. **Add protocol endpoint guard only after compile green.**
   - `/v1/chat/completions` accepts `openai_chat`, `local_openai_chat`, `custom_openai_chat`.
   - `/v1/completions` accepts `openai_completions`.
   - `/v1/embeddings` accepts `openai_embeddings`.
   - `/v1/messages` accepts `anthropic_messages`.
   - Wrong endpoint should return a useful error: public model name, configured protocol label, and expected path.

6. **Portal route wizard must send and display protocol.**
   - Provider template dropdown chooses reasonable defaults for protocol + auth mode, but user can override.
   - Credential input belongs in Model Route wizard, not Provider template.
   - Load Models button must call preview using unsaved base URL, protocol, auth mode, and provider key/no-auth.
   - Save/Edit must preserve the same row identity. Editing a route must not create a duplicate row.

## Acceptance gate before reporting done

Run these and paste the result into the next DeepSeek audit note:

```bash
cargo fmt --check
cargo check --locked --all-targets
cargo test --locked
python3 scripts/portal_smoke.py
```

If Playwright is available, also verify route wizard manually/automatically:

1. Login as admin.
2. Create Provider template `Local llama.cpp`, base URL `http://127.0.0.1:8088/v1`.
3. Create Model Route with protocol `local_openai_chat`, auth `none`.
4. Click Load Models and select `qwen3.8-flash-next`.
5. Save.
6. Edit the route, change a non-identity field, save.
7. Confirm the same row is updated, not duplicated.
8. Delete unused route/provider/team/API key and confirm UI + backend state both update.

## Notes

This audit supersedes any vague protocol TODO. The root cause is contract propagation plus protocol/backend-format confusion. Fix those directly; do not add Redis, sidecars, service discovery, or another config layer for this.
