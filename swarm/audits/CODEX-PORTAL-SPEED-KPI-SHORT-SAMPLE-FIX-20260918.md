# CODEX audit — Portal speed KPI must not promote short samples — 2026-09-18

## Problem

The dashboard hero previously showed the latest request `Tokens/sec` value as the primary `Speed` KPI. For tiny smoke requests with `total_ms < 100`, that produced values such as `89K*` in the top KPI area.

That is correct as a per-request diagnostic, but misleading as an overview KPI. A dashboard should not turn a sub-100ms smoke request into an apparent production speed claim.

## Fix

Changed `static/index.html`:

- Added `sustainedTokRate()` for token-rate samples where `total_ms >= 100`.
- Added `dashboardSpeed()` so the Admin dashboard hero uses only sustained token-rate samples.
- If only short samples exist, the hero now shows:
  - value: `Needs sample`
  - note: `run a longer request`
- Kept per-request logs unchanged: request rows still show compact `Tokens/sec` with `*` for short samples, such as `89K*`.
- Updated User dashboard aggregate `Tokens/sec` to ignore short samples and explain when the latest request is a short sample.

Updated `swarm/scripts/portal_full_page_audit.mjs`:

- Collects `.ops-status` hero KPI values.
- Fails if the dashboard hero Speed KPI displays a short-sample `*` value.
- Fails if the hero Speed note uses `short sample` copy instead of asking for a longer sample.

## Verified

Commands run against live HTTPS portal:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Result: all PASS.

Key evidence from `portal_full_page_audit`:

- Dashboard hero Speed: `Needs sample`
- Dashboard hero Speed note: `run a longer request`
- Recent requests still show: `TOKENS/SEC 89K*`

## Verdict

This fixes a production UX problem: the Portal still exposes token-rate diagnostics, but it no longer advertises fake-looking speed numbers from smoke-test-sized requests as the main dashboard KPI.
