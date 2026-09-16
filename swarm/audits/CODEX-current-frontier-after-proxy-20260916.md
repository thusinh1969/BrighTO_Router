# CODEX current frontier after proxy rewrite — 2026-09-16

Superseded note: after this file was written, DeepSeek fixed the handler/main/route integration enough for `cargo check --all-targets` to pass. Use `CODEX-post-compile-green-sota-audit-20260916.md` as the current source of truth. This file remains useful only as history/context.

Scope: current worktree at `/mnt/data02/BrigTO_Router` after `src/proxy/mod.rs` and `src/contract.rs` changed. I did not edit `src/`.

Rules for the next coder:

- Do not restore old `BudgetStore`, `BackendPool`, `Decision`, `RouteDecision`, or `SkipReason`.
- Do not make Redis part of v1 fastest/default production runtime.
- Do not keep two proxy implementations.
- Do not claim SOTA/fastest until compile/test gates pass and hot-path checks pass.

Commands run:

```bash
cargo check --all-targets
python3 - <<'PY'
import sqlite3, pathlib
sql=pathlib.Path('migrations/0001_init.sql').read_text()
conn=sqlite3.connect(':memory:')
conn.executescript(sql)
print(conn.execute("select name from sqlite_master where type='table' order by name").fetchall())
PY
rg -n "redis|REDIS|valkey|acquire_excluding|try_reserve|ledger_tx|resp\.bytes\(\)|reqwest::Client::new|Body::from_stream" src Cargo.toml docker-compose.yml .env.example docs -S
```

Observed:

- `cargo check --all-targets` fails.
- SQLite migration executes successfully and creates `api_keys`, `backends`, `model_routes`, `teams`, `usage_ledger`.
- `src/proxy/mod.rs` is now directionally correct: shared `state.client`, resolved backend key, `Body::from_stream`, `BudgetReservation`/`ConcurrencyGuard`/`BackendLease` carried by `CompletionReporter`.

## P0-1 — Compile frontier is integration, not contract rollback

Current compile errors:

```text
src/handlers.rs:19 unresolved imports BudgetStore, Decision, RouteDecision, SkipReason
src/handlers.rs:242 no method try_reserve; use reserve
src/handlers.rs:259 no field pool; AppState has backends
src/handlers.rs:311 no method commit on Arc<RamBudgetStore>
src/handlers.rs:338 no field ledger_tx; AppState has ledger
src/proxy/mod.rs:567/612/626/653/669 no method acquire_excluding on Arc<RamBackendPool>
src/budget/mod.rs:140 RpmBucket doesn't implement Debug
src/route/mod.rs:409 test Backend initializer missing api_key
```

This is not a reason to re-add the removed traits/enums. The new concrete RAII direction is the correct architecture for fastest+safe routing because it keeps budget/concurrency/backend leases alive until completion/abort. Re-adding the old trait API would reopen the TOCTOU and early-drop bugs already audited.

Fix once:

1. Add `RamBackendPool::acquire_excluding(&Arc<Self>, route, excluded)` in `src/route/mod.rs`, because `proxy` must retry without reusing a failed backend.
2. Rewrite `src/handlers.rs` to build `ProxyContext` and call `proxy_forward`; delete the old local `forward_to_backend` path.
3. Rewrite `src/main.rs` to instantiate the current concrete `AppState`.
4. Fix `RpmBucket` debug derive and the `Backend { api_key }` test initializer.

## P0-2 — `src/route/mod.rs` lacks the API that the new proxy requires

Evidence:

- `src/proxy/mod.rs:567`, `612`, `626`, `653`, `669` call `state.backends.acquire_excluding(&ctx.route, &tried)`.
- `src/route/mod.rs:108-146` only exposes `pub fn acquire(self: &Arc<Self>, route: &ModelRoute) -> Option<BackendLease>`.
- `src/route/mod.rs:149-230` has the right private `choose_candidate(route, excluded)` primitive, but no public wrapper that starts from a caller-owned exclude set.

Required behavior:

- normal selection considers `route.backend_ids`;
- retry excludes `tried`;
- `fallback_backend_id` is only attempted after primary candidates are unavailable or already tried;
- `BackendLease` is returned only after inflight is incremented;
- old `RouteDecision::Skip` must not come back.

Minimal patch shape, concrete enough to paste/adapt:

```rust
pub fn acquire(self: &Arc<Self>, route: &ModelRoute) -> Option<BackendLease> {
    self.acquire_excluding(route, &HashSet::new())
}

pub fn acquire_excluding(
    self: &Arc<Self>,
    route: &ModelRoute,
    excluded: &HashSet<i64>,
) -> Option<BackendLease> {
    let mut excluded = excluded.clone();
    if let Some(lease) = self.acquire_from_route(route, &mut excluded) {
        return Some(lease);
    }

    let fallback = route.fallback_backend_id?;
    if excluded.contains(&fallback) {
        return None;
    }

    // Fallback is deliberately phase 2. Do not mix it into primary least-load,
    // otherwise a quiet paid fallback can beat healthy primary GPUs.
    let fallback_route = ModelRoute {
        model_name: route.model_name.clone(),
        backend_ids: vec![fallback],
        fallback_backend_id: None,
        chars_per_token: route.chars_per_token,
        first_byte_timeout: route.first_byte_timeout,
    };
    self.acquire_from_route(&fallback_route, &mut excluded)
}

fn acquire_from_route(
    self: &Arc<Self>,
    route: &ModelRoute,
    excluded: &mut HashSet<i64>,
) -> Option<BackendLease> {
    loop {
        let candidate = self.choose_candidate(route, excluded)?;
        let Some(state) = self.states.get(&candidate.backend_id) else {
            excluded.insert(candidate.backend_id);
            continue;
        };

        if candidate.half_open
            && state
                .half_open_probe
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            excluded.insert(candidate.backend_id);
            continue;
        }

        let max = state.max_inflight.load(Ordering::Relaxed);
        let prev = state.inflight.fetch_add(1, Ordering::Relaxed);
        if max > 0 && prev >= max {
            state.inflight.fetch_sub(1, Ordering::Relaxed);
            if candidate.half_open {
                state.half_open_probe.store(true, Ordering::Release);
            }
            excluded.insert(candidate.backend_id);
            continue;
        }

        return Some(BackendLease {
            pool: Arc::clone(self),
            backend_id: candidate.backend_id,
        });
    }
}
```

This is the same body as current `acquire`, moved into a helper so retry and fallback can reuse it. Keep this local to `route`; no new traits and no Redis.

Acceptance:

```bash
rg -n "acquire_excluding" src/route/mod.rs src/proxy/mod.rs
cargo check --all-targets
```

## P0-3 — `src/handlers.rs` is still the old buffered router and must become thin glue

Evidence:

- `src/handlers.rs:18-20` imports removed symbols `BudgetStore`, `Decision`, `RouteDecision`, `SkipReason`.
- `src/handlers.rs:42-121` defines private metrics while canonical metrics are in `src/metrics.rs`.
- `src/handlers.rs:212` full-parses request body into `serde_json::Value`.
- `src/handlers.rs:242-257` calls old `try_reserve`.
- `src/handlers.rs:259-270` calls old `pool.pick`.
- `src/handlers.rs:277` reads `std::env::var(&backend.api_key_ref)` on the request path, despite `src/config/mod.rs:93` resolving backend keys into `Backend.api_key`.
- `src/handlers.rs:282-452` forwards directly with per-request client and buffered response.
- `src/handlers.rs:288` buffers upstream with `resp.bytes().await`.
- `src/handlers.rs:338` uses removed raw `ledger_tx`.
- `src/handlers.rs:346` returns `Body::from(upstream_body)`.
- `src/handlers.rs:446` creates `reqwest::Client::new()` per request.

Required patch shape:

```rust
use std::borrow::Cow;
use serde::Deserialize;
use crate::contract::{ApiKey, AppState, BudgetError, ModelRoute};
use crate::proxy::{ProxyContext, is_stream_request, proxy_forward};

#[derive(Deserialize)]
struct RequestProbe<'a> {
    #[serde(borrow)]
    model: Cow<'a, str>,
    #[serde(default)]
    stream: bool,
}

// after auth and to_bytes(...)
let probe: RequestProbe<'_> = serde_json::from_slice(&body_bytes)
    .map_err/return bad request;
let model = probe.model.into_owned();
let stream = probe.stream || is_stream_request(&body_bytes);

let reservation = match state.inner.budget.reserve(&key, &model, est_tokens) {
    Ok(r) => r,
    Err(BudgetError::RateLimited { retry_after }) => return 429 with Retry-After,
    Err(BudgetError::BudgetExceeded { remaining }) => return 429 budget exceeded,
};

let concurrency = match state.inner.budget.acquire_concurrency(&key) {
    Ok(g) => g,
    Err(()) => return 429 concurrency limited,
};

let proxy_req = Request::from_parts(parts, body_bytes);
let ctx = ProxyContext {
    api_key: key,
    model_name: model,
    route,
    request_id,
    stream,
    reservation: Some(reservation),
    concurrency: Some(concurrency),
    start: started,
};

proxy_forward(state.inner.clone(), proxy_req, ctx).await
```

Delete or stop compiling these old pieces after the rewrite:

- `forward_to_backend`
- `extract_usage`
- private `Metrics`
- old success/backend error counters in `RouterState`

Keep `estimate_tokens`, `extract_api_key`, `generate_request_id`, and `build_error` if still useful.

Acceptance:

```bash
rg -n "BudgetStore|Decision|RouteDecision|SkipReason|try_reserve|state\\.inner\\.pool|ledger_tx|forward_to_backend|reqwest::Client::new|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)" src/handlers.rs
```

Expected output: no hits except comments/tests explicitly marked obsolete.

## P0-4 — `src/main.rs` still builds old AppState and never warms runtime state

Evidence:

- `src/main.rs:14` imports removed `BackendPool` and `BudgetStore`.
- `src/main.rs:72` creates one ledger channel, but `LedgerWriter::run` now requires primary and overflow receivers.
- `src/main.rs:74-75` creates trait objects instead of concrete `Arc<RamBudgetStore>` and `Arc<RamBackendPool>`.
- `src/main.rs:77-82` initializes removed fields `pool` and `ledger_tx`, and omits required fields `backends`, `client`, `ledger`, `metrics`, `max_body_bytes`.
- `src/main.rs:63-70` loads/swaps config, but never calls `budget.load_teams(&snapshot.teams)` or `backends.upsert_backend(...)`.
- `src/config/mod.rs:212-216` only swaps `cfg`; runtime budget/backend state will stay stale after config reload unless main wraps reload with state sync.

Required patch shape:

```rust
let boot_snapshot = boot_loader.load_snapshot().await?;

let budget = Arc::new(RamBudgetStore::new());
budget.load_teams(&boot_snapshot.teams);

let backends = Arc::new(RamBackendPool::new());
for backend in boot_snapshot.backends.values().cloned() {
    backends.upsert_backend(backend);
}

cfg.store(Arc::new(boot_snapshot));

let client = reqwest::Client::builder()
    .pool_idle_timeout(Duration::from_secs(90))
    .tcp_keepalive(Duration::from_secs(60))
    .build()
    .context("build backend HTTP client")?;

backends.clone().start_health_loop(client.clone(), Duration::from_secs(5));

let metrics = brigto_router::metrics::Metrics::install();

let (ledger_tx, ledger_rx) = mpsc::channel(8192);
let (overflow_tx, overflow_rx) = mpsc::channel(8192);
let ledger = LedgerSink::new(ledger_tx, overflow_tx);
tokio::spawn(async move {
    if let Err(e) = writer.run(ledger_rx, overflow_rx).await {
        tracing::error!(error = %e, "ledger writer exited");
    }
});

let app_state = AppState {
    cfg: cfg.clone(),
    budget: budget.clone(),
    backends: backends.clone(),
    client,
    ledger,
    metrics,
    max_body_bytes,
};
```

For config polling, do not call old `DbConfigLoader::run` directly unless it also updates `budget` and `backends`. The simplest safe v1 shape is a helper in `main.rs`:

```rust
fn sync_runtime_snapshot(snapshot: &ConfigSnapshot, budget: &RamBudgetStore, backends: &RamBackendPool) {
    budget.load_teams(&snapshot.teams);
    for backend in snapshot.backends.values().cloned() {
        backends.upsert_backend(backend);
    }
}
```

Then the poll task must do:

```rust
match poll_loader.load_snapshot().await {
    Ok(snapshot) => {
        sync_runtime_snapshot(&snapshot, &budget, &backends);
        cfg.store(Arc::new(snapshot));
    }
    Err(e) => tracing::warn!(error = %e, "config reload failed"),
}
```

Do not add a callback framework. One helper is enough.

## P0-5 — Mechanical compile fixes

`src/budget/mod.rs:82`:

```rust
#[derive(Debug)]
struct RpmBucket {
```

`src/route/mod.rs:409-418` test helper must include the new runtime-only key field:

```rust
api_key: Some("test-backend-key".into()),
```

These two errors are mechanical. Fix them in the same integration patch.

## P1 — Redis/Valkey is currently over-engineering for fastest v1

Evidence:

- `Cargo.toml:34` depends on `redis`.
- `docker-compose.yml:9` sets `REDIS_URL`.
- `docker-compose.yml:15-17` makes router depend on `valkey`.
- `docker-compose.yml:35-44` starts `valkey`.
- `rg -n "redis::|Redis|REDIS|valkey" src` shows no runtime Redis use.
- `docs/llm-router-rust-plan.md:142-144` says Redis is only for hard global quota and adds latency/ops cost.

Verdict:

For SOTA fastest/default production, Redis should not be in the default binary or default compose. PostgreSQL is justified for config/admin/ledger background paths. Redis is justified only if product requirements demand strict cross-instance prepaid/global concurrency guarantees. Current repo has neither implemented that path nor proven it is required.

Fix once:

- Remove default `redis` dependency from `Cargo.toml`, or make it optional behind a disabled feature such as `hard-quotas-redis`.
- Remove `REDIS_URL`, `depends_on: valkey`, and `valkey` service from default `docker-compose.yml`.
- Keep one ADR note: “Redis is reserved for hard global quota mode; fastest profile uses per-instance RAM counters and accepts bounded soft-budget overshoot.”

Do not implement Redis now just because the crate is present. That would add a network hop to the hot path and contradict the fastest profile.

## P1 — Admin router exists but is not mounted

Evidence:

- `src/admin/mod.rs:222` exposes `pub fn router() -> Router`.
- `src/handlers.rs:124-133` defines runtime routes but does not nest `/admin`.
- `static/index.html` exists, but no route serves it.

Fix once after compile frontier:

```rust
Router::new()
    // existing LLM routes
    .nest("/admin", crate::admin::router())
    .route("/", get(serve_static_index))
```

Keep this behind the same binary only if admin is protected by master key + trusted network. Do not add React/build tooling.

## P1 — Admin IP allowlist trusts spoofable forwarding headers

Evidence:

- `src/admin/mod.rs:118-129` extracts client IP from `x-forwarded-for` or `x-real-ip`.
- `src/admin/mod.rs:171-197` enforces `ADMIN_ALLOW_CIDR` using that extracted value.

Verdict:

This is acceptable only when nginx or a trusted proxy strips and re-adds those headers before traffic reaches the router. If the router is exposed directly or behind an untrusted proxy, any client can spoof `x-forwarded-for: 127.0.0.1`.

Fix once:

- For v1: document and enforce deployment behind nginx/trusted network; do not expose admin publicly.
- Minimal code fix when touching admin: pass peer address via axum `ConnectInfo<SocketAddr>` or an extension from trusted ingress, and ignore `x-forwarded-for` unless a `TRUST_PROXY_HEADERS=true` env flag is set.

## Hot-path acceptance before claiming SOTA

Run after the integration patch:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
rg -n "reqwest::Client::new\\(|Client::builder\\(|std::env::var\\(&backend\\.api_key_ref\\)|serde_json::from_slice::<Value>\\(&body_bytes\\)|resp\\.bytes\\(\\)\\.await|Body::from\\(upstream_body\\)|ledger_tx|state\\.inner\\.pool|BudgetStore|BackendPool|RouteDecision|SkipReason|Decision" src
```

Expected final hot-path grep:

- no per-request client construction;
- no env/file/backend key lookup in handler/proxy request path;
- no full `serde_json::Value` parse of request body;
- no buffered streaming response;
- no raw ledger sender;
- no old trait/decision API.

Architecture verdict:

```text
Axum handler
  -> auth via ArcSwap ConfigSnapshot
  -> bounded Bytes body + tiny model/stream probe
  -> RamBudgetStore::reserve + acquire_concurrency (RAII)
  -> RamBackendPool::acquire_excluding (BackendLease RAII)
  -> shared reqwest::Client
  -> Body::from_stream with SSE usage tap
  -> LedgerSink.try_record -> background Postgres/file writer
```

PostgreSQL stays out of the request path. Redis stays out of the fastest/default profile. No additional service should be added until a production requirement proves it is needed.
