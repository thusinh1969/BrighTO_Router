# CODEX AUDIT — COMPILE BLOCKER + PROVIDER FLOW GATE

Date: 2026-09-17  
Role: Codex auditor. Product code remains DeepSeek-owned.

## Verdict

**NOT ACCEPTED. Do not say done. Do not build/push Docker from this worktree yet.**

Current worktree does not compile, so the new Provider → Load models → Test connection → Save UX cannot be proven by Docker or Playwright. This is the immediate root cause of “Portal still unchanged”: a fresh image cannot be built from the latest code while `src/admin/mod.rs` has incomplete response constructors.

## Hard blocker: Rust compile fails

Command run:

```bash
cargo check --workspace
```

Result:

```text
error[E0063]: missing fields `active_route_count`, `can_delete`, `delete_blockers` and 1 other field in initializer of `BackendResponse`
   --> src/admin/mod.rs:819:13

error[E0063]: missing fields `can_delete`, `disabled_reason`, `effective_enabled` and 1 other field in initializer of `RouteResponse`
    --> src/admin/mod.rs:1622:5
```

Exact files/functions:

- `src/admin/mod.rs:create_backend()` returns `BackendResponse` without the new lifecycle fields.
- `src/admin/mod.rs:route_response()` returns `RouteResponse` without the new lifecycle fields.

Required fix:

1. `cargo check --workspace` must pass before any Docker rebuild.
2. For `create_backend()`, either return a freshly reloaded/re-queried backend row or populate safe initial fields:
   - `active_route_count: 0`
   - `usage_count: 0`
   - `can_delete: true`
   - `delete_blockers: []`
3. For route create/patch, best fix is to return the row through the same list/read path that computes lifecycle. If using a constructor default, it must at least set:
   - `effective_enabled` according to route enabled + backend enabled
   - `disabled_reason`
   - `usage_count`
   - `can_delete`

## Lifecycle delete rules still not compliant

User-approved production rule:

- Provider can be deleted only when it has **no active model route** and **no transaction log**.
- Disabled routes must not block provider deletion.
- Provider disabled means all attached models become effectively disabled and cannot route traffic.
- Model route can be manually disabled.
- Model route cannot be deleted after transaction history exists; disable it instead.
- UI filters must support Enabled / Disabled / All.

Current static findings:

1. `delete_backend()` checks every route reference, regardless of `model_routes.enabled`. This is too strict. Disabled route should not block provider deletion.
2. `delete_backend()` checks `backend_ids` but does not check `fallback_backend_id`.
3. `delete_backend()` does not check `usage_ledger.backend_id`. This is too loose. Transaction history must block provider deletion.
4. `delete_route()` deletes immediately and does not check `usage_ledger.model`. This is too loose. Transaction history must block model delete.
5. UI delete button still calculates used providers with a client-side `usedIds` map from all routes, not server `can_delete/delete_blockers`. It will disagree with backend rules.

Required backend fix:

- Provider delete guard:
  - block if any **enabled** route references provider in `backend_ids` or `fallback_backend_id`.
  - block if `usage_ledger` has any row for `backend_id`.
  - allow delete only if both counts are zero.
- Route delete guard:
  - block if `usage_ledger` has any row where `model = model_name`.
  - allow delete if usage count is zero.
- Disable path:
  - keep PATCH support for `enabled=false` on providers/routes.
  - router runtime must refuse disabled routes and routes whose providers are disabled.

Required UI fix:

- Render server-computed fields: `effective_enabled`, `disabled_reason`, `usage_count`, `can_delete`, `delete_blockers`.
- Add filters: Enabled / Disabled / All for Providers and Models.
- Disable delete buttons based on `can_delete`, with tooltip/message from `delete_blockers`.

## Provider flow UX: mostly aligned, but Providers page still keeps old mental model

The new `Models → Add model` wizard is the right direction:

- Provider catalog dropdown.
- Base URL.
- API key in the model flow.
- Load models opens a model-picker modal.
- Test connection required before Save.
- Save auto-creates/reuses backend.

But the Providers page still exposes:

- `Add provider`
- `Edit`
- `Delete`
- reusable backend wording

This keeps the same confusion the user rejected. For the normal Admin UX, provider catalog should be fixed from `.env`/config and the user should create an enabled model route in one place.

Required UX simplification:

1. Make **Models → Add model** the only normal path to create a route/provider connection.
2. Rename Providers page to something like **Provider inventory** or hide it under Advanced.
3. Remove any `Load models` action from Provider rows. Loading models must happen inside the Add/Edit model route wizard because that is where the API key and custom URL live.
4. Provider page can show internal backends, status, active route count, usage count, enabled/disabled, and safe delete only. It must not teach users to create providers first.

## API key behavior needs one clear decision

The wizard currently accepts a provider API key and stores it as a route secret reference. That is acceptable for routing, but edit mode cannot show the existing provider key because only the ref is stored.

User previously said Admin can see keys again. Apply this consistently:

- Client API keys: already revealable in Admin table.
- Provider API keys: either revealable to Admin or explicitly documented as write-only provider secrets. Do not mix wording.

If revealable provider keys are required, implement a controlled Admin reveal endpoint for route/provider secret refs. If write-only is retained, remove wording that suggests Admin can inspect it later.

## Mandatory gate before reporting done

After fixing compile:

```bash
cargo check --workspace
cargo test --workspace
```

Then rebuild/recreate live container from the current worktree and prove:

1. `GET /admin/provider-catalog` returns only the fixed catalog entries from `.env`/fallback.
2. Fresh DB has 1 default team, 1 demo client key if desired, zero routes, zero usage, and only provider inventory rows if intentionally seeded.
3. Playwright, using a matching browser, executes:
   - login Admin
   - Models → Add model
   - choose `Custom LLM`
   - URL `http://127.0.0.1:8088/v1`
   - Load models opens picker
   - choose exactly one model
   - Test connection passes
   - Save succeeds
   - model appears in list
   - client call through router succeeds
   - edit model updates same row, not duplicate
   - delete model works before usage
   - after usage, delete is blocked and disable works
   - provider delete is blocked with active route or usage
   - provider delete works after no active route and no usage
4. Repeat one small smoke with DeepSeek using max output <= 8 tokens only after local path passes.

## Current status for Codex

Blocked at compile. I did not run Playwright against this worktree because that would test an older Docker image, not the current code.
