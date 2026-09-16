# CODEX live audit — route wizard partial, not accepted yet — 2026-09-17

Live URL checked:

```text
http://118.69.81.92:18080/
```

Runtime was rebuilt/restarted from current local source so browser refresh now sees the latest embedded Portal HTML.

## What is now visible live

The live Portal HTML now contains:

```text
Provider model — load then pick one
Load models
/admin/summary
Budget type
```

This means DeepSeek moved route creation in the right direction: the route modal is no longer only a raw manual backend-checkbox form.

## Still not accepted

The live Portal HTML still does **not** contain:

```text
Custom Provider
OpenAI-compatible
Anthropic Messages
Provider type
```

So the user-reported product logic is still incomplete.

## Required final wizard behavior

The Create Model Route flow must be a single professional flow:

1. Provider dropdown with predefined provider list and `Custom Provider` option.
2. Provider type dropdown:
   - `OpenAI-compatible`
   - `Anthropic Messages API`
3. Provider API key input inside this wizard when key is missing or Admin chooses update.
4. Save/update provider key via a write-only backend endpoint. Do not return provider plaintext key.
5. `Refresh models from provider` button inside this wizard.
6. Searchable loaded model list.
7. Exactly one provider model selected before Save.
8. Public route name auto-fills from selected provider model; Admin can edit alias.
9. Context/max output/input price/output price saved in same flow.
10. Route table shows provider + provider model + prices so Admin can verify.

## Current technical gates

These passed on current dirty worktree before runtime restart:

```text
cargo check --locked --all-targets
python3 scripts/hotpath_guard.py
python3 scripts/portal_smoke.py
node --check portal JS
```

Do not ask for final Playwright acceptance until the missing wizard pieces above exist.

## Secret/key testing instruction from user

User provided a DeepSeek key for later real-provider testing. Do not write it into audit docs, logs, README, screenshots, or final messages.

After Portal/API passes full audit, use the key only locally to configure DeepSeek and run small real tests:

```text
1k tokens
50k tokens
200k tokens
Provider/model: DeepSeek V4.0 Pro
```

Record only sanitized results: status, latency, token counts, memory, errors, and approximate cost if returned/known. Never print the secret.
