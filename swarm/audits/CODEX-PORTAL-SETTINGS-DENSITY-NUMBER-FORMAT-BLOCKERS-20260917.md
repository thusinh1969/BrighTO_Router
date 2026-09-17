# CODEX AUDIT — Portal settings, density, number format blockers

Date: 2026-09-17  
Role: Codex auditor/mentor only. No product code changed in this audit.  
Status: **FAIL — current portal still has product polish and lifecycle bugs.**  
This supersedes any earlier broad acceptance note for Portal UI. The current state is not yet acceptable for a professional open-source release.

## Current evidence

Observed current dirty work from DeepSeek:

- `src/admin/mod.rs`: API key list now exposes `revealable`.
- `static/index.html`: API key copy UI improved; provider `format` moved to a dropdown; table padding reduced from `11px 14px` to `7px 12px`.

These are useful fixes, but they do not close the main UX bugs below.

## Blocker 1 — CRUD render lifecycle is still broken

The root cause is unchanged: top-level `render(view)` clears `#content`, but CRUD handlers call section renderers directly, appending a second copy of the same screen.

Current offenders in `static/index.html`:

- `renderProviders($("content"))` after provider model load/save.
- `renderModels($("content"))` after route save/delete.
- `renderTeams($("content"))` after team save.
- `renderKeys($("content"))` after key create/disable.
- `renderUsage($("content"))` after usage filter apply.

Required fix:

- Add exactly one central refresh path, for example `rerenderCurrentView() { return render(currentView); }`.
- Replace every direct post-CRUD `renderX($("content"))` call with that central refresh path.
- Do not let any section renderer append to an already-populated `#content` after a CRUD action.
- After create/edit/delete/apply, Playwright must prove there is exactly one main panel for that view.

Acceptance test required:

1. Login admin.
2. Providers: create, edit, delete. After each action: exactly one Providers panel/table exists.
3. Model routes: create, edit, delete. After each action: exactly one Model routes panel/table exists.
4. Teams: create, edit, delete or disable if delete is intentionally unsupported. After each action: exactly one Teams panel/table exists.
5. API keys: create, reveal/copy, disable. After each action: exactly one API keys panel/table exists.
6. Usage: apply filters twice. After each action: exactly one Filters panel exists.

## Blocker 2 — Settings is not a real settings page

Current `renderSettings()` is read-only runtime information only. The user explicitly requires Settings to control Portal presentation because the current UI is too large and not professional enough for dense admin work.

Required Settings UI, minimal and no over-engineering:

- Add a `Portal preferences` panel above or below runtime settings.
- Font size selector:
  - `Small`
  - `Normal`
  - `Large`
- Density selector:
  - `Compact`
  - `Comfortable`
- Add `Reset to default`.
- Apply immediately without restart.
- Persist in `localStorage`, not PostgreSQL and not Redis.
- Defaults should fit an admin dashboard: `Normal` font + `Compact` density.

Implementation direction:

- Use CSS variables or root attributes, for example:
  - `<html data-font="normal" data-density="compact">`
  - `--body-font-size`, `--table-font-size`, `--cell-padding-y`, `--panel-padding`, `--card-number-size`.
- Add small pure JS helpers:
  - `loadUiPrefs()`
  - `saveUiPrefs(prefs)`
  - `applyUiPrefs(prefs)`
- Call `applyUiPrefs()` before first render, and after changes in Settings.

Acceptance test required:

1. Open Settings.
2. Select `Small` font and `Compact` density.
3. Verify computed body font/table font changed.
4. Verify table row height is smaller than before.
5. Refresh the browser with F5.
6. Verify the selected preferences persist.
7. Press reset.
8. Verify defaults restore.

## Blocker 3 — Number formatting is amateur and inconsistent

Current main formatter:

```js
function fmt(n){ if(n===null||n===undefined)return "-"; return Number(n).toLocaleString(); }
```

This prints raw large numbers such as `1,000`, `50,000`, `1,000,000`, `2,500,000,000`. For a router dashboard this is hard to scan. The chart formatter has a separate `fmtNum()` and uses lowercase `k`, which is inconsistent with the required `K/M/B` style.

Required behavior:

| Raw value | Display |
|---:|---:|
| 999 | `999` |
| 1,000 | `1K` |
| 1,234 | `1.2K` |
| 50,000 | `50K` |
| 999,999 | `999K` or `1M`, but choose one rule and document it in code comments |
| 1,000,000 | `1M` |
| 12,345,678 | `12.3M` |
| 1,000,000,000 | `1B` |
| 2,500,000,000 | `2.5B` |

Required fix:

- Replace the generic `fmt()` with explicit formatters by meaning:
  - `fmtCount(value)` for requests, tokens, byte counts, errors.
  - `fmtMoney(value)` or keep `fmtCost(value)` for dollars.
  - `fmtDuration(value)` or keep `fmtDur(value)` for latency.
- Use uppercase `K`, `M`, `B` everywhere: cards, tables, charts, logs, settings values.
- Do not use lowercase `k` in charts.
- For table cells where exact numbers matter, show compact text but keep exact value in `title`, for example display `12.3M` with tooltip `12,345,678`.

Acceptance test required:

- Seed or mock usage stats containing `999`, `1_234`, `50_000`, `1_000_000`, `2_500_000_000`.
- Verify dashboard cards, usage tables, chart axis labels, and logs display `K/M/B` consistently.
- Verify costs still display dollars, not K/M/B.
- Verify durations still display `ms`, `s`, or `m s`, not K/M/B.

## Blocker 4 — Visual density still wastes space

Current CSS evidence:

- `body` font: `14px`.
- top title: `26px`.
- card big number: `30px`.
- panel padding: `22px`.
- panel margin-bottom: `20px`.
- empty state padding: `40px 16px`.
- modal padding: `26px`.
- table padding now reduced to `7px 12px`, but the rest of the layout is still oversized.

Required target:

- Default admin view should show more data above the fold without feeling cramped.
- Compact density target:
  - body font about `13px`.
  - table font about `12.5px` to `13px`.
  - row height roughly `32px` to `36px`.
  - panel padding `14px` to `16px`.
  - panel gap `12px` to `14px`.
  - card big number `24px` to `26px`, not `30px`.
  - topbar title `22px` to `24px`, not `26px`.
  - empty state padding `22px` to `28px`, not `40px`.
- Comfortable density can keep larger spacing for users who prefer it.

Acceptance test required:

- Use Playwright to measure `.panel`, `td`, `.card .big`, `.topbar h2` computed styles under both Compact and Comfortable.
- The two modes must produce different measurable spacing/font sizes.
- Default should be Compact enough for production admin screens.

## Blocker 5 — Logs still emphasize weak admin metrics

Admin/User logs must highlight what matters for an LLM router:

- tokens in
- tokens out
- total tokens
- tokens per second (`tok/s`)
- cost
- provider
- route/model
- team
- key owner/prefix
- status/error
- duration as secondary context

Do not make giant raw millisecond values or unformatted counters the most visible part of the UI. Duration is useful, but tokens/sec and cost are more useful for router operations.

Required fix:

- Add/verify a visible `tok/s` column in request logs if backend has enough data to compute it.
- Display duration via `fmtDur()` only.
- Display token counts via `fmtCount()` only.
- Do not display huge raw millisecond numbers in dashboard cards.

Acceptance test required:

- Run mock calls with known input/output token counts and duration.
- Verify logs show compact tokens and computed `tok/s`.
- Verify no dashboard card displays raw `123456789 ms` style numbers.

## Non-negotiable release bar

The Portal cannot be called ready until all are true:

- Every CRUD flow is Playwright-tested through real clicks/forms, not just API calls.
- No duplicate panels after any create/edit/delete/apply action.
- Settings has working font size and density preferences with F5 persistence.
- All large counts use consistent uppercase `K/M/B` formatting.
- Logs prioritize `tok/s`, tokens, cost, provider, model, team, key, status.
- No PostgreSQL or Redis is added for UI preferences. Keep it local and simple.

