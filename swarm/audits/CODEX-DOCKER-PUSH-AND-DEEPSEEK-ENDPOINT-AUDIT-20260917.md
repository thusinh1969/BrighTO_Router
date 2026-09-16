# Docker push and DeepSeek endpoint audit - 2026-09-17

## Docker image now running

Codex built and pushed Docker image:

```text
thusinh1969/brighto_airouter:v1
Docker Hub digest: sha256:bcfb590338d27a6b6ea49a27d42a530972dc3265abe2eb383ed21eb0058a24c8
Local image id: sha256:113912d72d41747136e3e0b042a9d191eece13176613eeea1cef448620ad6e44
Built from clean committed source: d9de430 Polish portal and list model routes
```

The local stack was restarted with that image and verified on public/server IP:

```text
http://118.69.81.92:18080/healthz -> 200 ok
http://118.69.81.92:18080/readyz -> 200 ready
portal_new=True
/admin/backends -> 10 rows
/admin/routes -> 0 rows
```

Important: this running image includes the polished static Portal committed in `d9de430` and `GET /admin/routes`.

## Current worktree after DeepSeek edits

DeepSeek has additional uncommitted backend changes in:

```text
src/admin/mod.rs
src/handlers.rs
```

Those changes add:

- `GET /admin/teams`
- `GET /admin/keys`
- `GET /admin/stats`
- `/portal/me`
- `/portal/me/usage`
- `/portal/me/stats`
- mount `/portal` from `handlers::router`

These changes are not in the Docker image above because they are not committed yet.

## Verification on current uncommitted worktree

Codex audited by running:

```bash
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
./scripts/test_postgres.sh
```

Observed result:

```text
HOTPATH_GUARD_PASS
clippy pass
55 unit tests pass
4 integration tests pass
```

## Auditor verdict for DeepSeek

Current uncommitted backend endpoints are acceptable to continue frontend work, with two required follow-ups before commit/image rebuild:

1. Add a runtime smoke for `/portal/me`, `/portal/me/usage`, `/portal/me/stats` using a generated client API key. Unit tests are good, but user-facing endpoints need one real HTTP proof.
2. Ensure the final Portal UI clearly separates two login modes:
   - Admin mode: `ADMIN_MASTER_KEY`, can manage providers/routes/teams/keys.
   - User mode: BrighTO API key, can view own team/key/usage only.

Security boundaries observed in current diff:

- No provider plaintext key returned.
- `GET /admin/keys` returns key prefix only, not plaintext secret.
- `/portal/me*` filters by the authorized API key id.
- Hot path remains DB-free by guard.

Do not rebuild/push Docker image again until DeepSeek commits the finished frontend/backend bundle and the HTTP smoke above passes.
