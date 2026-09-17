# CODEX AUDIT — LIVE TLS Playwright CRUD audit FAIL

Date: 2026-09-17 10:00 ICT  
Role: Codex auditor/mentor. Runtime config changed to enable TLS; product code not changed.

## Verdict

**FAIL — Portal is visually improved, but current CRUD UX is buggy. Provider edit appears broken because the UI appends stale panels after save/delete instead of clearing and re-rendering the current view.**

This is not acceptable for SOTA admin UX. Fix this before more visual polish or provider testing.

## Runtime state

TLS is now enabled on the Docker runtime:

```text
LISTEN_ADDR=0.0.0.0:18443
BASE_URL=https://rtx3090:18443
TLS_CERT_PATH=/certs/fullchain.pem
TLS_KEY_PATH=/certs/privkey.pem
```

Verified:

```text
https://127.0.0.1:18443/healthz -> HTTP/2 200 ok
https://rtx3090:18443/healthz    -> HTTP/2 200 ok
https://rtx3090:18443/readyz     -> HTTP/2 200 ready
http://127.0.0.1:18080/healthz   -> connection refused (expected; HTTP port is off)
```

## TLS install/helper bugs found while enabling Docker TLS

### Bug 1 — `./start.sh tls` fails when cert/key are already in `ssl/`

Command:

```bash
./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host rtx3090 --port 18443
```

Failure:

```text
cp: 'ssl/fullchain.pem' and 'ssl/fullchain.pem' are the same file
```

Root cause: helper blindly copies source to the same destination.

Required fix:

- In `cmd_tls`, compare `realpath` of source and destination.
- If they are the same file, skip copy.
- Do not fail when the user already placed files exactly where docs told them to place files.

### Bug 2 — TLS key permission is wrong for Docker non-root runtime

Container runs as non-root user `router` UID `10001`. The helper/docs set:

```bash
chmod 600 ssl/privkey.pem
```

On host this makes the key readable only by host user, not UID `10001` inside the container. Router enters restart loop:

```text
Error: load TLS cert/key
Caused by:
    failed to read from file `/certs/privkey.pem`: Permission denied (os error 13)
```

Temporary runtime workaround applied on this machine:

```bash
chmod 644 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

Required production-quality simple fix:

- `start.sh tls` should make the key readable by the container user.
- Preferred when possible:

```bash
chown 10001:10001 ssl/privkey.pem ssl/fullchain.pem
chmod 600 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

- If `chown` is not permitted, print a clear fallback command and fail with an actionable message, or explicitly set `chmod 644` for local/dev self-signed certs with a warning.
- Docs must mention the non-root UID requirement.

## Playwright audit evidence

I ran live browser audits against the actual TLS Docker runtime, not a mock UI.

Artifacts:

```text
swarm/out/playwright/20260917-095414-live-tls-crud-audit/
swarm/out/playwright/20260917-095508-live-tls-crud-audit-127/
swarm/out/playwright/20260917-095602-live-tls-full-ui-audit/
swarm/out/playwright/20260917-095615-live-tls-full-ui-audit-rerun/
```

The final rerun used:

```text
BASE=https://127.0.0.1:18443
browser=Chromium via mcr.microsoft.com/playwright:v1.63.0-noble
TLS=self-signed, browser ignoreHTTPSErrors=true
```

Result:

```text
FAIL
```

## Primary UI blocker: CRUD appends duplicate panels

### User-visible symptom

After creating/editing/deleting Provider, the old table remains visible and a new table is appended below it. This makes Provider edit look like it did nothing. It also creates duplicate action buttons, so later clicks become ambiguous or fail.

Observed by Playwright:

```text
Provider create duplicates Provider panels: 2 panels after save.
Provider edit duplicates Provider panels: 2 panels after save, so old row remains visible and user thinks edit failed.
Provider delete duplicates Provider panels: 2 panels after delete.
Route create duplicates Model routes panels: 2 panels after save.
Route edit duplicates Model routes panels: 2 panels after save; this can look like duplicate row bug.
Route delete duplicates Model routes panels: 2 panels after delete.
Team create duplicates Teams panels: 2 panels after save.
Team edit duplicates Teams panels: 2 panels after save.
API key create duplicates API keys panels: 2 panels after Done/render.
API key disable duplicates API keys panels: 2 panels after disable.
Usage Apply duplicates filter panels: before=1, after=2.
```

### Root cause in `static/index.html`

`render(view)` clears content correctly:

```js
function render(view){
  var c=$("content"); c.innerHTML="";
  ...
}
```

But CRUD handlers bypass `render(view)` and call section renderers directly with the existing content node. Section renderers append panels, so stale panels remain.

Exact problematic lines/patterns:

```text
static/index.html:451  renderProviders($("content"));
static/index.html:473  renderProviders($("content"));
static/index.html:623  renderModels($("content"));
static/index.html:632  renderModels($("content"));
static/index.html:693  renderTeams($("content"));
static/index.html:782  renderKeys($("content"));
static/index.html:802  renderKeys($("content"));
static/index.html:837  renderUsage($("content"));
```

### Required one-pass fix

Add one central helper and use it everywhere after data changes or filter apply:

```js
function rerenderCurrent(){
  var cur=document.querySelector(".nav.active");
  var view=cur ? cur.dataset.view : "dashboard";
  render(view); // render() already clears #content
}
```

Then replace direct section calls:

```js
renderProviders($("content"));
renderModels($("content"));
renderTeams($("content"));
renderKeys($("content"));
renderUsage($("content"));
```

with:

```js
rerenderCurrent();
```

Or call `go(currentView)` if you want nav state reset. Do not call `renderX($("content"))` directly unless that function itself starts with `c.innerHTML = ""`.

Acceptance after fix:

- After Provider create: exactly 1 `Provider backends` panel.
- After Provider edit: exactly 1 `Provider backends` panel; old name not visible; edited name visible.
- After Provider delete: exactly 1 `Provider backends` panel; deleted row gone.
- Same invariant for Models & Routes, Teams, API Keys, Usage.

## Provider edit: backend works, UI makes it look broken

After re-navigation, Provider edit did persist correctly:

- Edited provider name visible.
- Base URL changed.
- Protocol display changed to `Anthropic Messages` after format changed to `anthropic`.

So the root cause is not the backend PATCH endpoint. The root cause is stale duplicate DOM after save.

Fix the re-render first. Then rerun Playwright before changing backend logic.

## Additional UI/product gaps found

### 1. Provider “Load models” quick-create path is stale and unsafe

`static/index.html:490`:

```js
routes=api("/admin/routes","GET");
```

Problems:

- Missing `await`.
- Does not re-render the route table.
- Creates a route from a model chip without the full Route Wizard fields: protocol, auth mode, route credential, pricing, context, max output.

Required fix:

- Remove quick-create from Provider Load Models modal, or make chip click open the full Route Wizard prefilled with provider + provider model.
- Do not create production routes from a shortcut that bypasses the SOTA route definition flow.

### 2. API Keys table auto-fetches every plaintext key on render and emits console noise for legacy keys

Current UI auto-calls `/admin/keys/{id}/reveal` for every key row. Browser console had repeated `410` responses for legacy keys.

Required fix:

- Show prefix and a `View` button by default.
- Reveal a single key only when admin clicks `View`.
- If you keep auto-reveal by user request, handle 410 without browser console noise and show `legacy/not stored` cleanly.

### 3. Team lifecycle has no Delete/Disable action in the table

Playwright found Team has Edit only. If this is intentional, label it clearly. If not, add a simple Disable button before release.

No overengineering: soft-disable is enough.

### 4. UI density is too airy for an admin console

Measured on live Portal:

```json
{
  "rowHeights": [39,43,42,39,43,42,39,43,43,42,39,43,43,42,39,46,46,46,46,45],
  "panelMargins": ["20px","20px","20px","20px","20px"],
  "panelPadding": ["22px","22px","22px"],
  "cardHeights": [126,126,126,126,126,126,126,126]
}
```

This looks polished but wastes vertical space. For real admin usage, make it denser:

- Table cell vertical padding: target 8–10px, row height about 34–38px.
- Panel padding: target 16px, not 22px.
- Panel margin/gap: target 12–14px, not 20px.
- Dashboard card height: target 92–108px, not 126px.
- Keep visual hierarchy, but reduce empty air between rows/cards.

## Cleanup performed

The live audit created test rows. I removed audit debris from the live DB:

```sql
DELETE FROM model_routes WHERE model_name LIKE 'pw-audit-%' OR model_name LIKE 'pwfull-%' OR model_name LIKE 'curl-audit-%';
DELETE FROM api_keys WHERE owner LIKE 'pw-audit-%' OR owner LIKE 'pwfull-%' OR owner LIKE 'curl-audit-%';
DELETE FROM teams WHERE name LIKE 'pw-audit-%' OR name LIKE 'pwfull-%' OR name LIKE 'curl-audit-%';
DELETE FROM backends WHERE name LIKE 'pw-audit-%' OR name LIKE 'pwfull-%' OR name LIKE 'curl-audit-%';
```

Post-cleanup health:

```text
https://127.0.0.1:18443/healthz -> ok
```

## Required acceptance gate after DeepSeek fix

Run these before claiming done:

```bash
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 scripts/portal_smoke.py
bash scripts/tls_smoke.sh
```

Then run a live TLS Playwright pass with these assertions:

1. Login admin over `https://127.0.0.1:18443` and `https://rtx3090:18443`.
2. F5 keeps session.
3. Provider create/edit/delete leaves exactly one Provider panel and no stale row.
4. Route create/edit/delete leaves exactly one Model routes panel and no stale row.
5. Team create/edit leaves exactly one Teams panel; disable/delete policy is clear.
6. API key create/reveal/disable leaves exactly one API keys panel.
7. Usage Apply leaves exactly one Filters panel.
8. Browser console has no unexpected 4xx/5xx noise except intentional wrong-password test.
9. Table/card spacing is reduced to compact admin-console density.

## Final instruction to DeepSeek

Fix the render lifecycle root cause first. Do not patch Provider edit in isolation. The same bug pattern exists across Provider, Route, Team, Key, and Usage screens.
