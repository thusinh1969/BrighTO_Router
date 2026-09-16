# Key reveal override audit - 2026-09-17

User clarified again:

```text
Không cần key 1 lần, Admin, cho họ thấy lại không sao. Tôi báo Deepseek rồi
```

## Current DeepSeek implementation state

Current uncommitted changes add:

```text
migrations/0003_admin_key_reveal.sql
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_secret TEXT;

GET /admin/keys/{id}/reveal
create_key stores plaintext in key_secret for new keys
GET /admin/settings
```

Validation already run by Codex auditor on current worktree:

```text
node --check /tmp/brighto-current-portal.js -> pass
python3 scripts/hotpath_guard.py -> HOTPATH_GUARD_PASS
cargo check --locked --all-targets -> pass
./scripts/test_postgres.sh -> 55 unit + 4 integration pass
```

## Product verdict

Backend direction matches the user's override, but the product is not aligned yet.

### P0 mismatch: UI still says key is shown once

Current `static/index.html` still shows this after creating a key:

```text
This plaintext key is shown <b>only once</b>. Store it securely.
```

This contradicts the user override. Remove this wording.

Correct wording:

```text
Admin can reveal this key again from the API Keys screen.
```

### P0 mismatch: API Keys table has no reveal action

Current API Keys table columns:

```text
ID, Prefix, Owner, Team, Allowed models, Enabled, Disable
```

It must include a reveal/copy action:

```text
ID, Prefix, Key, Owner, Team, Allowed models, Expiry, Budget, Enabled, Actions
```

Minimum action:

```text
Reveal
Copy
Disable
```

`Reveal` should call:

```text
GET /admin/keys/{id}/reveal
```

If the endpoint returns 410, show:

```text
This key was created before key reveal was enabled. Disable it and create a new key.
```

Do not fake the full key from prefix/hash.

### P0 mismatch: smoke test still encodes old security rule

Current `scripts/portal_smoke.py` still checks:

```text
admin /keys never returns plaintext
```

That old rule is no longer sufficient. Replace with tests for the new rule:

```text
POST /admin/keys returns lc- key
GET /admin/keys returns list rows without accidental full key unless deliberately designed otherwise
GET /admin/keys/{id}/reveal returns the same lc- key for admin
GET /admin/keys/{old_id}/reveal returns 410 when key_secret is NULL
bad admin key cannot reveal
user API key cannot call admin reveal
```

Important: even if `GET /admin/keys` later includes plaintext for convenience, make it an explicit product decision and test it. Safer UX is a `Reveal` button so keys do not appear in tables/screenshots by default.

## Security scope after user override

Allowed:

```text
Admin can reveal BrighTO client API keys.
```

Still forbidden unless user explicitly changes it:

```text
Provider LLM API keys shown in plaintext
Plaintext keys in logs
Plaintext keys in usage ledger
Plaintext keys in benchmark artifacts
Plaintext keys in user portal for other users
```

## Production tradeoff to document

Current implementation stores plaintext client API keys in `api_keys.key_secret`.

This is acceptable only if the project intentionally chooses simple open-source mode. It should be documented clearly in `SECURITY.md` or `README.md`:

```text
Admin key reveal stores client API keys so admins can view them again. Protect PostgreSQL and ADMIN_MASTER_KEY. Enterprise mode should use encryption or a managed secret store.
```

Do not pretend this is equivalent to hash-only storage.

## Acceptance checklist for DeepSeek before commit

```text
UI no longer says shown once
API Keys table has Reveal/Copy
Reveal calls GET /admin/keys/{id}/reveal
410 old-key state is handled cleanly
portal_smoke.py verifies reveal behavior
bad admin key cannot reveal
client user key cannot reveal through admin endpoint
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
./scripts/test_postgres.sh
cargo build --release --locked
python3 scripts/portal_smoke.py
```
