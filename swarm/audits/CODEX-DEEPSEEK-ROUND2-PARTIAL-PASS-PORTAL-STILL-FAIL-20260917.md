# CODEX AUDIT — DeepSeek round 2 partial pass, Portal still fails release bar

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal on `https://127.0.0.1:18443`  
Verdict: **PARTIAL PASS / STILL FAIL**

DeepSeek's `DEEPSEEK-SELF-AUDIT-ROUND2-20260917.md` is directionally useful but overclaims. The stale render / duplicate panel issue is fixed. The Portal is still not professional enough for release because settings, compact number formatting, and density controls remain missing.

## What is now fixed

Runtime Playwright evidence from `bash swarm/scripts/portal_polish_audit.sh`:

| Check | Result |
|---|---:|
| Provider panels after create | `1` |
| Provider panels after edit | `1` |
| Provider panels after delete | `1` |

This confirms the stale render duplicate-panel bug is fixed in practice. Codex updated the static gate so direct `renderX($("content"))` calls are accepted when `renderX(c)` clears `c.innerHTML` before appending DOM.

## What still fails

Current reusable static gate:

```bash
python3 swarm/scripts/portal_static_gate.py
```

Current result:

```text
PORTAL_STATIC_GATE FAIL 5
FAIL has Portal preferences settings :: missing literal: Portal preferences
FAIL has font-size preference state :: missing data-font/dataset.font or Small/Normal/Large labels
FAIL has density preference state :: missing data-density/dataset.density or Compact/Comfortable labels
FAIL has compact count formatter :: missing fmtCount/formatCompact
FAIL chart formatter uses uppercase K :: lowercase k marker present
PASS post-action renderProviders cannot append stale panels
PASS post-action renderModels cannot append stale panels
PASS post-action renderTeams cannot append stale panels
PASS post-action renderKeys cannot append stale panels
PASS post-action renderUsage cannot append stale panels
```

Runtime Playwright evidence still fails with these bugs:

- `fmt(1000)` returns `1,000`; required `1K`.
- `fmt(50000)` returns `50,000`; required `50K`.
- `fmt(1000000)` returns `1,000,000`; required `1M`.
- `fmtNum(1000)` returns `1.0k`; required uppercase `1K` or `1.0K` consistently.
- Dashboard big number is still `30px`; compact admin default should be about `24px` to `26px`.
- Panel padding is still `22px`; compact admin default should be about `14px` to `16px`.
- Settings page is still runtime read-only only; it has no Portal preferences.
- Usage still shows comma-formatted token counts such as `251,336`, `251,288`, `251,230`, `200,058`, `50,058` instead of compact `251.3K`, `200.1K`, `50.1K`.

## Required next DeepSeek fix

Do one focused UI pass. Do not touch Rust hot path.

1. Add `Portal preferences` to Settings:
   - Font size: `Small`, `Normal`, `Large`.
   - Density: `Compact`, `Comfortable`.
   - Reset to default.
   - Persist in `localStorage` only.
   - Apply via root attributes or CSS variables, for example `document.documentElement.dataset.font` and `dataset.density`.

2. Add a single compact count formatter:
   - `999 -> 999`
   - `1000 -> 1K`
   - `1234 -> 1.2K`
   - `50000 -> 50K`
   - `1000000 -> 1M`
   - `12345678 -> 12.3M`
   - `1000000000 -> 1B`
   - Use uppercase `K/M/B` everywhere.
   - Keep money and duration formatters separate.

3. Apply compact counts consistently:
   - Dashboard cards.
   - Usage summary cards.
   - Usage grouped tables.
   - Request logs token columns.
   - Chart axis labels.
   - Settings byte counts when large.

4. Make compact density the default for admin:
   - Card big number about `24px` to `26px`.
   - Panel padding about `14px` to `16px`.
   - Panel gap about `12px` to `14px`.
   - Table rows about `32px` to `36px`.
   - Keep `Comfortable` mode for larger spacing.

## Acceptance command

DeepSeek must run both:

```bash
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
```

Passing CRUD alone is not enough. Portal remains **not accepted** until both gates pass and the live Docker UI reflects the fixes.
