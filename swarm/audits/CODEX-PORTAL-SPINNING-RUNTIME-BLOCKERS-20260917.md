# CODEX AUDIT — Portal spinning: two concrete runtime blockers

Date: 2026-09-17 05:22 ICT  
Role: Codex auditor/mentor only — runtime workaround applied, no product code changed.

## Verdict

**BLOCKER. Portal is not reliably usable yet.**

There are two separate bugs:

1. **Runtime hang after startup when any backend is enabled.**
2. **Portal/API contract mismatch: Portal calls GET `/admin/teams` and GET `/admin/keys`, but router returns 405 in the running image.**

Both must be fixed before UI work can be accepted.

## Evidence

### 1. Process was alive but HTTP handler stopped responding

Observed from the machine:

```text
/mnt/data02/BrigTO_Router/target/release/brighto-router (deleted)
LISTEN 0.0.0.0:18080
```

`curl http://127.0.0.1:18080/healthz` connected but received no bytes until timeout. Because `/healthz` only returns static `"ok"`, this means the runtime/accept path was blocked; it was not a DB query or Portal JavaScript issue.

Docker router then also entered `unhealthy` with health checks timing out after 3s.

### 2. Health-loop root cause in code

`src/route/mod.rs:439-457` currently holds a `DashMap` entry guard across an `.await`:

```rust
if let Some(state) = self.states.get(&id) {
    ...
    let ok = check_backend_health(client, &base).await;
    self.note_result(id, ok);
}
```

This is not safe. The `state` guard remains alive while awaiting network I/O, then `note_result()` re-enters `self.states.get(&id)`. With `DashMap`/sync locks inside async runtime, this can block a Tokio worker. On this machine it made `/healthz` hang even though the process was still listening.

### 3. Portal API mismatch

The Portal HTML calls:

```text
GET /admin/backends
GET /admin/routes
GET /admin/teams
GET /admin/keys
```

Live results after restarting the container:

```text
GET /admin/backends -> 200 OK
GET /admin/routes   -> 200 OK
GET /admin/teams    -> 405 Method Not Allowed, allow: POST
GET /admin/keys     -> 405 Method Not Allowed, allow: POST
```

So the dashboard spins/fails because two initial data calls cannot succeed.

Current source shows duplicate route definitions:

```rust
.route("/teams", post(create_team))
.route("/keys", post(create_key))
...
.route("/teams", get(list_teams))
.route("/keys", get(list_keys))
```

Do not rely on duplicate path registration. Combine methods on one route.

## Runtime workaround I applied for the user's immediate browser test

I disabled all enabled backends in the local PostgreSQL DB:

```sql
UPDATE backends SET enabled = false WHERE enabled = true;
```

Then I force-removed/restarted the router container. After that:

```text
GET http://127.0.0.1:18080/healthz -> 200 OK
GET http://127.0.0.1:18080/readyz  -> 200 OK
GET http://127.0.0.1:18080/         -> 200 OK
GET http://rtx3090:18080/healthz    -> 200 OK
```

Container state became healthy.

This is only a workaround. If any backend is re-enabled before the health-loop bug is fixed, the server can hang again.

## One-pass fix DeepSeek should do

### A. Fix health loop without adding architecture

Change `health_check_all()` so no map guard survives across `.await`.

Correct shape:

```rust
async fn health_check_all(&self, client: &reqwest::Client) {
    let targets: Vec<(i64, String)> = self.states
        .iter()
        .filter_map(|entry| {
            let id = *entry.key();
            let state = entry.value();
            if !state.enabled.load(Ordering::Relaxed) {
                return None;
            }
            let base = state.base_url.read().ok().and_then(|url| url.clone())?;
            if base.is_empty() { None } else { Some((id, base)) }
        })
        .collect();

    for (id, base) in targets {
        let ok = check_backend_health(client, &base).await;
        self.note_result(id, ok);
    }
}
```

Also add a regression test if practical: enable one backend whose `/health` times out, run health loop once under timeout, and verify `/healthz`/a trivial spawned future is not starved. Keep it simple.

### B. Fix duplicate Axum route definitions

Change admin router declarations to one route per path:

```rust
.route("/teams", get(list_teams).post(create_team))
.route("/keys", get(list_keys).post(create_key))
```

Remove the later duplicate `.route("/teams", get(...))` and `.route("/keys", get(...))` lines.

### C. Fix admin IP for LAN testing

Current default is `ADMIN_ALLOW_CIDR=127.0.0.1/32` in code if env is missing. For this test machine, `.env` should include a clear LAN/dev value, for example:

```env
ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0
```

For production docs, tell users to restrict this to office/VPN CIDRs or put Portal behind a reverse proxy/SSO later.

### D. HTTPS clarity

The running service on port `18080` is HTTP. `https://rtx3090:18080/` will not work unless the binary actually starts TLS on that listener.

Immediate expected URL for local testing:

```text
http://rtx3090:18080/
```

If TLS direct mode is implemented, it should be a separate explicit config path using `TLS_CERT_PATH` and `TLS_KEY_PATH`, and the startup log must print whether it is serving HTTP or HTTPS.

## Acceptance gate

After the fix, verify from this machine:

```bash
cargo fmt --check
cargo check --locked --all-targets
cargo test --locked
python3 scripts/portal_smoke.py

ADMIN_KEY=<from .env, do not print it>
curl --noproxy '*' -fsS http://127.0.0.1:18080/healthz
curl --noproxy '*' -fsS http://rtx3090:18080/healthz
curl --noproxy '*' -fsS -H "x-admin-key: $ADMIN_KEY" http://127.0.0.1:18080/admin/teams
curl --noproxy '*' -fsS -H "x-admin-key: $ADMIN_KEY" http://127.0.0.1:18080/admin/keys
```

Expected:

```text
healthz -> ok
/admin/teams -> JSON array
/admin/keys -> JSON array
container health -> healthy after 15s
```

Then use Playwright/browser:

1. Open `http://rtx3090:18080/`.
2. Login with admin key.
3. Dashboard finishes loading without spinner.
4. Provider/Route/Team/API Key tables all populate.
5. Refresh page; login state persists if localStorage session has been implemented.

## Do not ship until fixed

Do not continue UI styling work on top of a hanging runtime and broken admin list endpoints. These are root-cause blockers and should be fixed first.
