# DeepSeek portal HTTP smoke verified - 2026-09-17

Codex auditor verified current HEAD after DeepSeek portal/backend commit and Codex visual references.

Current HEAD:

```text
431ba2e Add visual references for portal and benchmarks
6071c41 Portal polish: OpenRouter-like UI, self-service /portal/me endpoints, HTTP smoke, AGENT.md
```

Checks run:

```bash
node --check /tmp/brighto-current-portal.js
python3 scripts/hotpath_guard.py
cargo check --locked --all-targets
./scripts/test_postgres.sh
cargo build --release --locked
python3 scripts/portal_smoke.py
```

Observed result:

```text
HOTPATH_GUARD_PASS
cargo check pass
55 unit tests pass
4 integration tests pass
release build pass
PASS admin /teams lists 1 team
PASS admin /keys starts empty
PASS admin /stats starts empty
PASS POST /admin/keys returns lc- key
PASS GET /portal/me returns own key + team
PASS GET /portal/me/usage returns own row
PASS GET /portal/me/stats aggregates own usage
PASS bad key -> 401
PASS admin /keys never returns plaintext
RESULT PASS
```

Auditor verdict:

- The new admin/user portal backend endpoints are green by compile, unit/integration tests, and real HTTP smoke.
- Security invariant holds in smoke: `/admin/keys` does not return plaintext API keys.
- User self-service is correctly separate from admin auth in smoke: `/portal/me*` uses client API key and rejects bad key with 401.
- Hot path guard still passes.

Remaining before pushing a new Docker image for this latest HEAD:

1. Confirm the Portal UI from `static/index.html` visually in browser after DeepSeek's latest commit.
2. Build and push `thusinh1969/brighto_airouter:v1` from HEAD if the user wants the latest DeepSeek Portal live in Docker.
