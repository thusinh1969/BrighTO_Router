# Key visibility user override - 2026-09-17

User override:

```text
Không cần key 1 lần, Admin, cho họ thấy lại không sao.
```

Meaning for DeepSeek:

Admin Portal may show BrighTO client API keys again after creation. The old requirement `plaintext key shown once only` is cancelled for admin users.

## Important root cause

Current schema stores client API keys as:

```text
api_keys.key_hash
api_keys.key_prefix
```

A hash cannot be reversed. Existing keys created before a storage change cannot be shown again because the plaintext was never stored.

If Admin must see API keys again, DeepSeek must change storage intentionally. Do not fake this in the UI.

## Minimal implementation options

Preferred production-safe option:

```text
api_keys.key_ciphertext TEXT
API_KEY_ENCRYPTION_SECRET in .env / Kubernetes secret
```

Behavior:

- On key creation, generate plaintext key.
- Store `key_hash` for fast auth lookup as today.
- Store encrypted plaintext in `key_ciphertext` for Admin Portal reveal.
- Admin `GET /admin/keys` may include revealed key or a separate `GET /admin/keys/{id}/reveal` endpoint.
- User Portal must still show only its own key/prefix unless user requirement changes.
- Never log plaintext key.
- Never include plaintext key in benchmark artifacts, usage rows, or request logs.

Simpler open-source option if user accepts lower security:

```text
api_keys.key_plaintext TEXT
```

This is easiest but weaker for production. If chosen, document it clearly as open-source/simple mode and recommend encryption for production/enterprise.

## Backward compatibility

Existing rows without stored plaintext/ciphertext must show:

```text
Not recoverable; rotate/create a new key to make it revealable.
```

Do not display a fake key from prefix/hash.

## UI requirement

API Keys screen should show:

```text
Team
Owner
Key / reveal action
Prefix
Allowed models
Expiry
Budget
RPM limit
Concurrency limit
Enabled
Disable/Revoke
```

Admin can copy the key again. Make this obvious and safe.

## Scope note

This override is about BrighTO client API keys unless the user explicitly says provider LLM API keys should also be revealable. Provider keys remain sensitive and should still avoid plaintext display by default.
