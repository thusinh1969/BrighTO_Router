# CODEX AUDIT — Anthropic live smoke PASS

Date: 2026-09-17 09:55 ICT  
Role: Codex auditor/mentor. No product code changed.

## Verdict

**PASS. Anthropic Messages path is live-tested through BrighTO-Router.**

The user confirmed `ANTHROPIC_API_KEY` is exported from `~/.bashrc`. I used it only inside a subprocess, did not print it, and did not write it to tracked files.

## Test scope

This was a tiny paid-provider smoke, not a benchmark and not a stress test.

Temporary isolated runtime:

- Temporary PostgreSQL container.
- Current `target/release/brighto-router`.
- Temporary data directory for route credential file.
- Admin key generated for the smoke only.
- No dev DB mutation.
- No tracked source changes.

## Result

```text
PASS create anthropic backend template
PASS anthropic model-list preview
PASS create anthropic route with route credential
PASS create client API key
PASS anthropic messages through router
PASS endpoint guard rejects anthropic route on chat endpoint
RESULT PASS
```

## What was verified

1. **Provider template creation**
   - Backend format `anthropic` accepted.
   - Base URL `https://api.anthropic.com` accepted.

2. **Model-list preview**
   - `POST /admin/routes/preview-models` with:
     - `protocol: anthropic`
     - `auth_mode: anthropic`
     - provider key from environment
   - Returned HTTP 200 and a non-empty `data[].id` model list.

3. **Route-level credential**
   - Created route with:
     - public model `anthropic-smoke`
     - provider protocol `anthropic_messages`
     - auth mode `anthropic`
     - provider key stored as route credential reference
   - Route creation returned HTTP 200.

4. **Anthropic Messages proxy**
   - Client called BrighTO-Router endpoint:

```text
POST /v1/messages
```

   - Router forwarded to Anthropic and returned HTTP 200.
   - Response included Anthropic usage fields with small token counts.

5. **Endpoint guard**
   - Calling the same Anthropic route via OpenAI Chat endpoint:

```text
POST /v1/chat/completions
```

   - Returned HTTP 400 with clear message telling client to call `/v1/messages`.

## Security note

The Anthropic API key was never printed in command output or committed. The temporary router log contained only harmless warnings about `env:NONE`; no provider key appeared in the log.

## Remaining interpretation

This closes the previous non-blocking gap: “Anthropic model-list/live protocol not independently tested.”

Current provider live coverage now includes:

- Local llama.cpp no-auth OpenAI-compatible chat.
- DeepSeek OpenAI-compatible route credential path.
- Anthropic Messages route credential path.

Do not expand paid-provider testing into large prompts. Large stress/benchmark remains mock/local only unless explicitly approved.
