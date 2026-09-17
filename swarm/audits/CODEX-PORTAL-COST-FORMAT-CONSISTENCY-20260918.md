# Codex audit — cost display consistency

Date: 2026-09-18
Scope: Dashboard and request cost formatting.

## Problem found

The Portal used mixed zero-cost formatting. Empty Dashboard showed `$0.00`, while request and active Dashboard views showed `$0.0000`. That small inconsistency made spend cards look less deliberate, especially because BrighTO tracks low per-request LLM spend.

## Fix applied

- Standardized `fmtCost(0)` to `$0.0000`.
- Kept non-zero values below one dollar at four decimals.
- Added a polish audit assertion for `fmtCost(0)`.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_polish_audit`: PASS, 0 failures, 0 bugs, 0 console errors.
- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_empty_state_audit`: PASS, 0 failures, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence:

```text
fmtCost0=$0.0000
```
