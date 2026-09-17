# Codex audit — robust Portal copy actions

Date: 2026-09-18
Scope: Portal copy buttons for endpoint, cURL, model names, and API keys.

## Problem found

Copy actions called `navigator.clipboard.writeText()` directly. In browsers or deployment contexts where the Clipboard API is unavailable or rejects permission, the copy action could fail without a useful fallback. This affects important actions such as copying the router endpoint, cURL command, model names, and API keys.

## Fix applied

- Added a small `copyText(text, label)` helper.
- Primary path uses `navigator.clipboard.writeText()`.
- Fallback path uses a temporary readonly textarea and `document.execCommand("copy")`.
- Failure path shows a clear toast: `Copy failed — select and copy manually`.
- Rewired endpoint, cURL, model-name, and key copy buttons to use the helper.
- Added static gate checks to prevent direct clipboard calls from returning.
- Added full-page Playwright coverage that forces `navigator.clipboard.writeText()` to reject and verifies fallback copy succeeds.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_user_journey_audit`: PASS, 0 failures, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence:

```text
forced navigator.clipboard failure -> textarea fallback copied copy-fallback-probe
copy buttons use fallback helper: PASS
copy actions do not call clipboard directly: PASS
```
