# DeepSeek handoff — front-end continuation — 2026-09-17

Role split from now:

- Codex stays auditor/reviewer through `swarm/audits/*`.
- DeepSeek continues implementation, especially portal/front-end UX.
- Keep the default production runtime lean: Rust router + PostgreSQL. No Redis in the default path unless strict multi-instance quota is proven by a production requirement and benchmark evidence.

## Current authoritative state

Repository:

```text
https://github.com/thusinh1969/Brighto_AIRouter
```

Current pushed HEAD when this handoff was written:

```text
2a0c0cd Add CI badge to README
```

Recent commits in order:

```text
1e96ca3 Initial public release cleanup
236aeb0 Add honest benchmark strategy and ledger lag gate
e70b9bf Clarify benchmark docs and fix large mock payloads
86fc652 Record benchmark smoke and Docker evidence
c372f23 Polish public install and provider setup
8d244ed Harden install env parsing and test database isolation
a63017a Make benchmark warmup strict and smoke output clear
57c4529 Clean public comments and legacy default handling
c45c8d0 Add CI for core checks
2a0c0cd Add CI badge to README
```

Official Docker image:

```text
thusinh1969/brighto_airouter:v1
```

Last pushed digest from the runtime-changing build:

```text
sha256:af58af787cbb7d659caa0054dd1c1ed895a95fac8f73f7c5b31f2a4b7d091c11
```

After that digest, only scripts/docs/comments/CI changed; Rust runtime source did not change.

## What Codex completed

### Repo/public surface

- Root was cleaned for open-source use.
- Old build/audit material lives under `swarm/`.
- README was rewritten as a professional public landing page.
- Long details were split out:
  - `INSTALL.md`
  - `PROVIDERS.md`
  - root `BENCHMARK.md`
  - `benchmarks/README.md`
  - `benchmarks/BENCHMARK.md`
  - `benchmarks/STRATEGY.md`
- GitHub Actions CI was added at `.github/workflows/ci.yml`.
- README has a CI badge.

### Install and operations

Default first run is now:

```bash
./start.sh install
./start.sh status
```

Default local portal:

```text
http://127.0.0.1:18080/
```

Default local admin key for first-time clone:

```text
brightoIsGreat@2026
```

Default local PostgreSQL port is `55432`, not `5432`, to avoid common host DB collisions.

`start.sh` now supports:

```bash
./start.sh install
./start.sh install --database-url 'postgres://user:pass@host:5432/brighto_router'
./start.sh install --k8s --replicas 2
./start.sh install --database-url 'postgres://user:pass@host:5432/brighto_router' --k8s --replicas 2
./start.sh set-key openai sk-...
./start.sh seed
./start.sh smoke
./start.sh gate
```

`start.sh` does not source `.env`; it parses only the variables it needs. This avoids executing API-key text or shell metacharacters from `.env`.

`make test` now uses `scripts/test_postgres.sh`, which starts a temporary PostgreSQL container unless the user explicitly provides a non-default `DATABASE_URL` or `TEST_DATABASE_URL`. This avoids test pollution of the local development DB.

### Database seed

`./start.sh install`, `./start.sh start`, `./start.sh restart`, and `./start.sh seed` run migrations and seed defaults.

Seed file:

```text
scripts/seed_defaults.sql
```

Default seed creates:

| Record | State |
|---|---|
| `Default Team` | enabled |
| OpenAI provider template | disabled |
| Anthropic provider template | disabled |
| Gemini provider template | disabled |
| DeepSeek provider template | disabled |
| Kimi provider template | disabled |
| Qwen provider template | disabled |
| Z.AI provider template | disabled |
| OpenRouter provider template | disabled |
| Meta Muse provider template | disabled |
| Custom OpenAI-compatible provider template | disabled |
| Client API keys | none |
| Model routes | none |

Provider secrets stay in `.env`. The DB stores only references such as `env:OPENAI_API_KEY`.

### Admin API added for portal/front-end

Current admin endpoints relevant to frontend:

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/admin/backends` | List provider backends. Shows `key_resolved`; never returns provider key plaintext. |
| `PATCH` | `/admin/backends/{id}` | Update name/base URL/key ref/weight/max inflight/format/enabled, then reload config immediately. |
| `GET` | `/admin/backends/{id}/models` | Fetch provider model list when the provider exposes OpenAI-style `data[].id`. |
| `POST` | `/admin/routes` | Create or update a model route, then reload config immediately. |
| `POST` | `/admin/teams` | Create team. |
| `PATCH` | `/admin/teams/{id}` | Update team. |
| `POST` | `/admin/keys` | Create client API key. Plaintext key is returned once. |
| `DELETE` | `/admin/keys/{id}` | Disable client API key. |
| `GET` | `/admin/usage` | Usage lookup. |

Current embedded portal:

```text
static/index.html
```

Current portal is intentionally simple: provider list/edit/fetch-model/create-route/team/key/usage on one static page. It is functional enough for install smoke, but it is not a polished product UI yet.

### Provider URL handling

Proxy and admin model-fetch now support both host-only and SDK-style provider base URLs:

| Base URL | Incoming path | Forwarded path |
|---|---|---|
| `https://api.openai.com` | `/v1/chat/completions` | `https://api.openai.com/v1/chat/completions` |
| `https://api.moonshot.ai/v1` | `/v1/chat/completions` | `https://api.moonshot.ai/v1/chat/completions` |
| `https://example.com/compatible-mode/v1` | `/v1/models` | `https://example.com/compatible-mode/v1/models` |

### Benchmark/docs

Benchmark docs now explain the terms instead of using unexplained jargon.

Important wording:

`1k c=50 offered rate 4000 RPS` means:

- payload is about 1,000 input tokens;
- at most 50 requests are active at the same time;
- the load generator tries to start up to 4,000 requests per second.

This is not a global standard. The defensible benchmark standard is direct-backend versus router on the same machine, same payload, same concurrency, same duration, same logging, and same network path.

Benchmark harness now supports:

- 1k, 50k, 200k release payloads;
- 500k and 1M stress payloads;
- router RSS memory sampling;
- true B10 ledger lag from router completion to PostgreSQL insertion;
- raw `oha` JSON artifacts for direct and router paths;
- baseline bootstrap and regression checking;
- strict warm-up command failure instead of silent warm-up errors.

Full gate was intentionally stopped by user on 2026-09-17. It only reached partial output:

```text
1k c=1 overhead p50 +0.241ms p99 +0.312ms
```

Do not use that interrupted run as release proof.

Latest completed smoke evidence before this handoff:

```text
1k c=50 overhead p50 +0.333ms p99 +0.478ms
50k c=50 overhead p50 +0.480ms p99 +0.609ms
200k c=50 overhead p50 +0.866ms p99 +0.816ms
B10 ledger lag p99 0.995060s rows 200/200 rps 209.56 non200 0
smoke command pass: True
release thresholds: not enforced in smoke; run BASELINE_BOOTSTRAP=1 ./start.sh gate for release proof
```

## Validation already completed

These checks have passed during the last Codex pass:

```bash
python3 -m py_compile scripts/bench_real.py benchmarks/make_payloads.py scripts/hotpath_guard.py
bash -n start.sh scripts/test_postgres.sh benchmarks/gate.sh
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
make test
docker compose config
./start.sh seed
./start.sh start
./start.sh status
./start.sh smoke
```

Observed install/runtime evidence:

```text
default_team_rows=1
provider_template_rows=10
enabled_provider_templates=0
model_route_rows=0
api_key_rows=0
healthz: ok
readyz: ready
admin_backend_rows=10
expected_provider_templates_present=true
```

## DeepSeek next work: frontend/portal

Goal: make first-time setup feel production-grade without adding a heavy front-end stack unless there is a clear reason.

Recommended scope:

1. Improve `static/index.html` into a clean setup wizard:
   - Step 1: enter admin key.
   - Step 2: show seeded providers and key status.
   - Step 3: paste/set provider key instructions using `./start.sh set-key`.
   - Step 4: enable provider.
   - Step 5: fetch models or type model manually.
   - Step 6: create route.
   - Step 7: create team API key and show it once.
2. Add a route-list endpoint if the portal needs to display existing model routes. Current API can create/update routes but does not list them.
3. Add client-side validation before calling admin API:
   - missing admin key;
   - backend ID missing;
   - model name empty;
   - backend IDs empty or invalid;
   - JSON budget parse errors with clear messages.
4. Keep provider API keys out of the browser response. The frontend may display `key_resolved=true/false` only.
5. Do not add React/Vite/Tailwind just for this page. A static HTML/CSS/JS page is enough unless UI scope grows materially.
6. Keep terms human-readable. If using an acronym, define it the first time.

Acceptance checks for DeepSeek frontend work:

```bash
node --check /tmp/extracted-portal.js
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
make test
./start.sh restart
curl -fsS http://127.0.0.1:18080/healthz
curl -fsS http://127.0.0.1:18080/readyz
curl -fsS -H "x-admin-key: <ADMIN_MASTER_KEY>" http://127.0.0.1:18080/admin/backends
```

If DeepSeek adds admin endpoints, require integration tests or at least focused unit tests for validation and immediate reload semantics.

## Hard boundaries

Do not add these while doing frontend polish:

- Redis in the default runtime.
- Prompt logging.
- Semantic cache.
- Provider key plaintext returned by admin API.
- Database reads in the request hot path.
- Full JSON parse of large messages in the request hot path.
- A heavy frontend build system without a concrete product reason.

## Remaining release evidence before public speed claims

Not done yet:

1. Full release baseline candidate:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
```

2. 500k and 1M stress proof on the intended dual-Xeon server:

```bash
BENCH_PAYLOADS=500k,1m \
BENCH_STREAM_PAYLOADS=500k-stream,1m-stream \
CONCS=1,50,200 \
RUNS=3 \
DUR=60s \
WARM=15s \
BENCH_B6=0 \
BENCH_B10=0 \
REQUIRE_PASS=0 \
python3 scripts/bench_real.py
```

3. Same-machine comparison against named routers before claiming “fastest in the world.”

Until those artifacts exist, phrase performance as “low-overhead Rust router with transparent benchmark artifacts,” not as a final global ranking.
