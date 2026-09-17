# CODEX audit — Tokens/sec must not disappear for short requests

Date: 2026-09-17

## Verdict

Fixed a frontend-only observability bug.

The Rust admin API already returns `total_tokens_per_second` for very fast requests by clamping positive-token 0ms samples to 1ms. The Portal then hid all token-rate values when `total_ms < 100`, so local/mock smoke calls showed `Tokens/sec` as `—`. That removed one of the most important admin signals during testing.

## Change

- `static/index.html`
  - `Tokens/sec` now renders whenever the API returns a finite token-rate value.
  - Requests shorter than 100ms are marked with `*`, for example `89K*`.
  - Tooltip explains that `*` is a very short sample and sustained tests should be used for benchmark claims.
  - Usage helper copy now says `* marks a very short sample`.
- `swarm/scripts/portal_polish_audit.mjs`
  - Gate now fails if the API returns `total_tokens_per_second` but the UI renders `—`.
  - Gate now requires `*` on sub-100ms token-rate samples.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_polish_audit.sh` — PASS
  - Log: `swarm/out/portal_polish_audit-tokens-sec-short-sample-232640.log`
  - Evidence: API returned `total_tokens_per_second: 89000`; UI expected/rendered `89K*`.
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-tokens-sec-short-sample-232724.log`
- `bash swarm/scripts/portal_user_journey_audit.sh` — PASS
  - Log: `swarm/out/portal_user_journey_audit-tokens-sec-short-sample-232724.log`

## Screenshot evidence

- `swarm/out/playwright/20260917-232724-portal-full-page-audit/desktop-1440-usage.png`
