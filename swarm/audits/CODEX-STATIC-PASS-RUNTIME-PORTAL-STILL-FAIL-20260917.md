# CODEX AUDIT — Static gate passed, live Portal still fails polish gate

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal on `https://127.0.0.1:18443`  
Verdict: **STATIC PASS / RUNTIME FAIL**

DeepSeek has now fixed the static markers and CRUD duplicate-panel problem. Good progress. Do not stop here: the live Portal still does not meet the OpenRouter/Hermes-style professional bar.

## What now passes

Current source gate:

```bash
python3 swarm/scripts/portal_static_gate.py
```

Result:

```text
PORTAL_STATIC_GATE PASS
```

Runtime CRUD evidence from Playwright:

| Check | Current result |
|---|---:|
| Provider panels after create | 1 |
| Provider panels after edit | 1 |
| Provider panels after delete | 1 |

Settings page now contains Portal preferences, and chart axis now uses uppercase K/M/B.

## What still fails in live runtime

Runtime gate:

```bash
bash swarm/scripts/portal_polish_audit.sh
```

Latest artifact:

```text
swarm/out/playwright/20260917-112747-portal-polish-audit/summary.json
```

Result: **FAIL**.

Current live evidence:

| Check | Current live value | Required |
|---|---:|---:|
| `fmt(1000)` | `1,000` | `1K` or no count callsite may use `fmt()` |
| `fmt(50000)` | `50,000` | `50K` or no count callsite may use `fmt()` |
| `fmt(1000000)` | `1,000,000` | `1M` or no count callsite may use `fmt()` |
| default density | `comfortable` | `compact` |
| dashboard big number | `30px` | about `24px` to `26px` in default compact |
| panel padding | `22px` | about `14px` to `16px` in default compact |
| Settings persistence | no localStorage key observed after initial apply | preferences must persist after F5 |
| Usage token counts | `251,336`, `251,288`, `251,230`, `200,058`, `50,058` | `251.3K`, `200.1K`, `50.1K` style |

## Root cause to fix

DeepSeek added `fmtCount()`, but count callsites still use old `fmt()` in Dashboard and Usage tables/logs. Adding a formatter function is not enough; every count/tokens/request/errors/bytes callsite must use it.

DeepSeek added density controls, but default is still `comfortable`; the user explicitly wants the admin Portal less huge by default. Set default to `compact` and make sure computed styles change immediately.

DeepSeek added preferences UI, but the runtime gate did not observe a localStorage key after initial apply. Ensure defaults are saved or preferences are persisted after first user interaction, and add a Playwright step proving F5 persistence.

## Required next patch

1. Replace count callsites:
   - Dashboard request/token cards: `fmtCount`.
   - Usage summary cards: `fmtCount`.
   - Usage grouped tables: `fmtCount` for requests/input/output/errors.
   - Request logs: `fmtCount` for IN/OUT tokens.
   - Settings max body bytes: `fmtCount` or explicit byte formatter with K/M/B.
   - Keep `fmtCost` for money and `fmtDur` for duration.

2. Fix default Portal preferences:
   - default font: `normal`.
   - default density: `compact`.
   - save/apply preferences through one helper.
   - F5 must keep selected font/density.

3. Fix compact CSS:
   - default compact must produce panel padding 14-16px.
   - card big number 24-26px.
   - table rows about 32-36px.
   - topbar title 22-24px.

## Acceptance

DeepSeek must pass both:

```bash
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
```

Current status: **not accepted**.
