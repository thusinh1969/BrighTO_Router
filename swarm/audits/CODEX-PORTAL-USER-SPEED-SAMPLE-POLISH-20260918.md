# CODEX audit — User Portal speed sample polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: User Portal dashboard Last 30 days speed KPI

## Root cause

When a user had only a very short request sample, the User Portal dashboard showed `Tokens/sec` as `—` even though the request log below had a measured token-rate value marked with `*`. This could look like speed logging was broken.

## Fix applied

- User dashboard now shows `Sample*` for Tokens/sec when only a short token-rate sample exists.
- The explanatory copy remains: `short sample; use a longer call for speed`.
- The detailed request log still shows the measured value with `*`.
- No benchmark claim is made from sub-100ms samples.
- Added a user journey audit assertion so short-sample data cannot regress to `Tokens/sec —`.

## Verification

Command run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Result:

- `portal-user-journey-audit`: PASS

Visual evidence reviewed:

- `swarm/out/playwright/20260918-032836-portal-user-journey-audit/mobile-390-user-dashboard.png`
