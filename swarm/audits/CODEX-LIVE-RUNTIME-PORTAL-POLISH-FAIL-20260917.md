# CODEX LIVE RUNTIME AUDIT — Portal polish still FAILS

Date: 2026-09-17  
Runtime tested: Docker service on `https://127.0.0.1:18443`  
Method: Playwright Chromium against live HTTPS portal, admin login, real DOM/computed style checks, provider CRUD clicks.  
Artifact: `swarm/out/playwright/20260917-100555-codex-live-polish-audit-noscreenshot/summary.json`  
Verdict: **FAIL — do not call Portal release-ready yet.**

This is not a source-only complaint. The live Docker runtime currently fails the professional dashboard bar requested by the user.

## Runtime evidence

Playwright result: `FAIL`.

Formatter evidence from live browser JS:

| Function | Output now | Required |
|---|---:|---:|
| `fmt(1000)` | `1,000` | `1K` |
| `fmt(50000)` | `50,000` | `50K` |
| `fmt(1000000)` | `1,000,000` | `1M` |
| `fmtNum(1000)` | `1.0k` | `1K` |
| `fmtNum(1000000)` | `1.0M` | `1M` |

Density evidence from live browser computed styles:

| Element | Current live value | Required direction |
|---|---:|---|
| body font | `14px` | configurable Small/Normal/Large |
| topbar title | `26px` | lower in compact mode |
| card big number | `30px` | about 24-26px in compact mode |
| panel padding | `22px` | about 14-16px in compact mode |
| panel margin | `20px` | about 12-14px in compact mode |
| dashboard card height | `126` | lower in compact mode |

Settings text in runtime is still only:

```text
Runtime settings /  / Read-only. Change these via environment variables and restart. /  / Router address / 0.0.0.0:18443 / Database / connected / Config reload / healthy / Max body bytes / 67,108,864 / Version / 0.1.0
```

There is no `Portal preferences` panel, no font size selector, no density selector, no localStorage preference key, and no root `data-font` / `data-density` attribute.

Provider CRUD runtime evidence:

| Flow | Panel count after action | Required |
|---|---:|---:|
| Provider create | `2` | `1` |
| Provider edit | `2` | `1` |
| Provider delete | `1` | `1` |

Usage/logs excerpt proves `TOK/S` now exists, but token counts are still raw comma numbers instead of compact `K/M/B`:

```text
Filters / MODEL / STATUS / All statuses / Success / Error / PROVIDER / All providers / openai / anthropic / gemini / deepseek / kimi / qwen / zai / openrouter / meta-muse / custom-openai / local-llama-qwen / crud-provider-1789593542-edited / Local Qwen / local-llama-qwen-audit / Local Qwen / DeepSeek V4 Pro / Local Qwen / Local Qwen / DeepSeek V4 Pro / Local Qwen / pw-polish-provider-4357575-edited / TEAM / All teams / Default Team / crud-team-1789593542-edited / local-llama-audit / KEY / All keys / lc-5c405 / lc-c03d9 / lc-a4bc1 / lc-55973 / lc-a96e5 / lc-04bbf / lc-c8130 / FROM / TO / RANGE / 7 days / 30 days / 90 days / Apply / Requests / 5 / Total tokens / 251,336 / Input / Output / 251,288 / 48 / Error rate / 0% (0) / Tokens by model / 0 / 62.8k / 125.7k / 188.5k / 251.3k / 09-16 / qwen-local / qwen3.8-flash-next / By model / MODEL	REQUESTS	INPUT	OUTPUT	ERRORS / qwen3.8-flash-next	4	251,230	32	0 / qwen-local	1	58	16	0 / By team / TEAM	REQUESTS	INPUT	OUTPUT	ERRORS / local-llama-audit	3	251,174	24	0 / Default Team	2	114	24	0 / By A
```

## Bugs DeepSeek must fix now

- Main formatter still does not use K/M/B: fmt(1000)=1,000
- Chart formatter still uses lowercase k: fmtNum(1000)=1.0k
- Dashboard big-number font too large: 30px
- Panel padding too airy: 22px
- Settings has no Font size selector: Small / Normal / Large.
- Settings has no Density selector: Compact / Comfortable.
- Provider create leaves 2 Provider panels; expected exactly 1.
- Provider edit leaves 2 Provider panels; expected exactly 1.

## Required fixes, one pass

### 1. One renderer lifecycle

Do not call section renderers directly after CRUD. Replace every post-action call like:

```js
renderProviders($("content"));
renderModels($("content"));
renderTeams($("content"));
renderKeys($("content"));
renderUsage($("content"));
```

with one central path:

```js
async function rerenderCurrentView(){ await render(currentView); }
```

or clear `#content` before any section renderer. The cleaner fix is central rerender. Acceptance: after every create/edit/delete/apply, exactly one panel for that view remains.

### 2. Settings must control Portal display

Add to Settings:

- `Portal preferences` panel.
- Font size: `Small`, `Normal`, `Large`.
- Density: `Compact`, `Comfortable`.
- `Reset to default`.
- Persist in `localStorage` only. Do not use PostgreSQL or Redis for display preferences.
- Apply before first render and immediately after user changes.

Use root attributes or CSS variables:

```js
document.documentElement.dataset.font = prefs.font;
document.documentElement.dataset.density = prefs.density;
```

Default should be `Normal` font and `Compact` density.

### 3. One compact formatter for counts

Current runtime still uses `fmt()` with `toLocaleString()`. Replace with one consistent count formatter and use it everywhere counts appear.

Required display examples:

| Raw | Display |
|---:|---:|
| 999 | `999` |
| 1,000 | `1K` |
| 1,234 | `1.2K` |
| 50,000 | `50K` |
| 1,000,000 | `1M` |
| 12,345,678 | `12.3M` |
| 1,000,000,000 | `1B` |
| 2,500,000,000 | `2.5B` |

Use uppercase `K/M/B` everywhere: cards, tables, charts, logs, settings. Costs remain dollars. Durations remain `ms/s/m`.

### 4. Reduce default visual density

Compact default target:

- panel padding: 14-16px.
- panel gap: 12-14px.
- card big number: 24-26px.
- topbar title: 22-24px.
- table rows: roughly 32-36px.
- empty states: 22-28px vertical padding.

Comfortable can keep larger values, but default admin view must be scan-friendly.

## Acceptance gate before claiming PASS

Run a Playwright test against the live Docker portal and attach the result. It must prove:

1. Settings preferences exist and persist after F5.
2. `Small/Normal/Large` changes computed font size.
3. `Compact/Comfortable` changes computed spacing/row height.
4. `fmtCount` style output appears on dashboard, Usage tables, chart axis, and request logs.
5. Provider/route/team/key/usage flows leave exactly one view panel after CRUD/apply.
6. No raw huge dashboard values like `251,336` where compact `251.3K` is expected.

Until this passes, Portal remains below the OpenRouter/Hermes-style professional bar.


## Poll update — backend gate green, Portal blockers unchanged

Time: 2026-09-17 around 10:10 local time.

Current repo state after DeepSeek commit `736f9b3` and Codex audit commit `da4eb4b`:

- `git status`: clean.
- `cargo fmt --check`: PASS.
- `cargo check --locked --all-targets`: PASS.
- Docker runtime health: `https://127.0.0.1:18443/healthz` returns `200 ok`.
- Static source still contains direct post-CRUD renderer calls:
  - `renderProviders($("content"))`
  - `renderModels($("content"))`
  - `renderTeams($("content"))`
  - `renderKeys($("content"))`
  - `renderUsage($("content"))`
- Static source still contains `function fmt(n){ ... Number(n).toLocaleString(); }`.
- Static source still contains chart formatter lowercase `k`.
- Static source still has no `Portal preferences`, `data-font`, `data-density`, or `fmtCount` marker.

Verdict for DeepSeek: Rust/backend build is not the current blocker. The release blocker is Portal product quality. Fix the UI root causes listed above before running another acceptance claim.


## Poll update — reusable acceptance gate added

Time: 2026-09-17 around 10:08 local time.

Codex added a reusable auditor-only gate:

```bash
bash swarm/scripts/portal_polish_audit.sh
```

Latest gate artifact:

```text
swarm/out/playwright/20260917-100824-portal-polish-audit/summary.json
```

Latest result: **FAIL**.

Current failing checks from the reusable gate:

- Main formatter still prints `1,000`, `50,000`, `1,000,000` instead of `1K`, `50K`, `1M`.
- Chart formatter still prints lowercase `1.0k` instead of uppercase `1K`.
- Dashboard big-number font is still `30px`.
- Panel padding is still `22px`.
- Settings still has no `Small / Normal / Large` font selector.
- Settings still has no `Compact / Comfortable` density selector.
- Provider create/edit still leaves duplicate Provider panels.
- Usage still displays comma-formatted large token counts instead of compact `K/M/B`.

DeepSeek must run this gate against live Docker before claiming Portal PASS. Passing backend tests alone is insufficient for this Portal objective.


## Poll update — staged gate now active in the 10-minute poller

Time: 2026-09-17 around 10:11 local time.

Codex added and activated a cheap source-first gate:

```bash
python3 swarm/scripts/portal_static_gate.py
```

The versioned 10-minute poller now runs gates in this order:

1. `cargo check --locked --all-targets`.
2. `python3 swarm/scripts/portal_static_gate.py`.
3. HTTPS health check.
4. `bash swarm/scripts/portal_polish_audit.sh` only when the static gate passes.

Current poller output:

```text
PORTAL_STATIC_GATE FAIL 10
SKIP: static gate failed; fix source blockers before running browser acceptance
```

This is intentional. Browser acceptance should not be used to claim success while the source still lacks the required Portal preferences, compact formatter, uppercase `K/M/B`, and central render lifecycle.
