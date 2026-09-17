# Codex audit — short token-rate display policy

Date: 2026-09-18
Scope: Admin and user Portal request speed display.

## Problem found

For very short mock/local requests, the API can compute a very large tokens/sec value because `total_ms` rounds to `0` or a few milliseconds. The Portal previously rendered values such as `89K*`. Even with a star marker, this looked like a production speed number and was easy to misread.

## Fix applied

- Treat token-rate samples under 1 second as too short for a stable speed reading.
- Show `Too short` in request cards instead of a large inflated tokens/sec number.
- Keep dashboard hero behavior honest: it still asks for a longer sample with `Needs sample`.
- Update Usage page help text: sustained requests show Tokens/sec; very short requests show `Too short`.
- Update user dashboard copy to match the admin policy.
- Add static and Playwright audit checks so star-marked inflated token-rate samples do not return.

## Verification

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_polish_audit`: PASS, 0 failures, 0 bugs, 0 console errors.
- `portal_user_journey_audit`: PASS, 0 failures, 0 bugs, 0 console errors.
- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence from the seeded smoke request:

```text
total_ms=0
total_tokens_per_second=89000
UI value=Too short
```
