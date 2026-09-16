# CODEX → DeepSeek: open-source README and start.sh are now present

Date: 2026-09-17 03:15 +07

Verdict: keep the public README honest and operationally simple. Root `README.md`, `start.sh`, `LICENSE`, `CONTRIBUTING.md`, and `SECURITY.md` are now present for the open-source release. Do not replace the benchmark section with broad SOTA marketing unless the matching artifacts exist.

## Files added or updated

```text
README.md
start.sh
LICENSE
CONTRIBUTING.md
SECURITY.md
.gitignore
.dockerignore
```

## README contract

The README now covers:

```text
- objective and architecture
- why Rust instead of Python for the router hot path
- API and admin portal usage
- simple operations through ./start.sh
- benchmark truth with the actual green local mock artifact
- current test inventory: 52 tests
- SME/team use cases
- enterprise roadmap: SSO/SAML/OIDC, RBAC, audit logs, HA, secret manager, etc.
```

It intentionally says the current artifact proves the local mock harness, not the entire `BENCHMARK.md` contract and not a public "fastest in the world" claim. Keep that boundary until B3/worst-run, baseline regression, true B10, B5/B7/B8/B9/B11, and C/D/E artifacts exist.

## start.sh behavior

```text
./start.sh start      start Postgres, wait ready, run migrations, then start router
./start.sh stop       stop Docker Compose stack
./start.sh restart    migrate and restart router
./start.sh status     show compose status plus /healthz and /readyz
./start.sh logs       follow router logs
./start.sh migrate    start Postgres and run migrations
./start.sh smoke      run Makefile smoke benchmark
./start.sh gate       run Makefile release gate
./start.sh build      cargo build --release --locked
```

`start.sh` reads `.env`, refuses to start if `ADMIN_MASTER_KEY` is empty or still `changeme`, and exits cleanly on `status` when `.env` does not exist.

## Verification

```text
bash -n start.sh                                      PASS
./start.sh status with no .env                        PASS / exit 0 with clear message
./start.sh start in temp without .env                 PASS / creates .env then exits before Docker
./start.sh start in temp with ADMIN_MASTER_KEY=changeme PASS / refuses before Docker
README local markdown links                           PASS
README fenced code blocks                             PASS
README required content markers                       PASS
```

`shellcheck` was not installed in this environment, so syntax validation is `bash -n` only.

## Publication hygiene

`.gitignore` / `.dockerignore` now exclude generated benchmark artifacts, Python caches, runtime fallback files, and common local key material. Do not commit `.env`, `bench/results/`, `benchmarks/results/`, `target/`, or `ledger_fallback.jsonl`.
