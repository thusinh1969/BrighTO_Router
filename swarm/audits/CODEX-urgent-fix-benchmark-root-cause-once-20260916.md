# URGENT — FIX BENCHMARK ROOT CAUSE ONCE, NOT SYMPTOMS

Time: 2026-09-16 22:35 ICT.  
Scope: current worktree after DeepSeek benchmark patch.  
Codex rule in this repo: I do not edit `src/`; this is the exact repair card for DeepSeek.

## Verdict

Current benchmark patch is closer, but the repo is still **not in a publishable SOTA state**. The immediate root cause is not Python syntax anymore. The root cause is that the benchmark stack still allows invalid/unbounded measurement states and the new mock binary is not clean under the repo's own `-D warnings` gate.

Fix this as one coherent patch. Do not publish fastest/SOTA numbers until all acceptance checks at the bottom pass on the real worktree.

## Evidence from current worktree

```text
$ python3 -m py_compile scripts/bench_real.py
PASS

$ cargo fmt --all -- --check
PASS

$ CARGO_INCREMENTAL=0 cargo check --all-targets
PASS

$ CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
FAIL
error: the `Err`-variant returned from this function is very large
  --> src/bin/mock_upstream.rs:78:6
   |
78 | ) -> Result<(u64, u64, u64, u64, Option<u64>, bool), Response> {
   |      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the `Err`-variant is at least 128 bytes

error: this `if` statement can be collapsed
   --> src/bin/mock_upstream.rs:132:9

error: this `if` statement can be collapsed
   --> src/bin/mock_upstream.rs:161:9
```

Current source inspection also shows:

```text
benchmarks/gate.sh:14  thr() { python3 -c "import tomllib,..."; }
scripts/bench_real.py:231-233  B6 still calls oha_run(ROUTER, "1k", "200", ...) with no target rate
scripts/bench_real.py:91-94    oha JSON is read without rejecting errorDistribution / empty statusCodeDistribution / null percentiles
src/ledger/mod.rs:50-60        ledger drop logging is now rate-limited; keep this, it is the right direction
```

## Root cause 1 — new mock upstream breaks the repo quality gate

File: `src/bin/mock_upstream.rs`.

This is a release blocker because `make check` / CI uses `cargo clippy --all-targets -- -D warnings`. A benchmark-only binary is still part of `--all-targets`; if it is red, the benchmark stack cannot be treated as canonical.

Apply the mechanical fix, not `#[allow]` unless there is a measured reason.

Required patch shape:

```rust
async fn common(
    headers: &HeaderMap,
    st: &S,
    body: &Bytes,
) -> Result<(u64, u64, u64, u64, Option<u64>, bool), Box<Response>> {
    // ...
    return Err(Box::new(
        StatusCode::from_u16(code as u16)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    ));
}
```

Then unwrap the boxed response at both call sites:

```rust
let (p, n, delay, _, fail_after, no_usage) = match common(&headers, &st, &body).await {
    Ok(v) => v,
    Err(r) => return *r,
};

let (p, n, delay, _, fail_after, _) = match common(&headers, &st, &body).await {
    Ok(v) => v,
    Err(r) => return *r,
};
```

Collapse both nested `fail_after` checks:

```rust
if let Some(f) = fail_after
    && i >= f
{
    std::process::abort();
}

if let Some(f) = fail_after
    && i >= f + 2
{
    std::process::abort();
}
```

Run `cargo fmt --all` after this. Expected result: clippy must be green with no allowances added for these warnings.

## Root cause 2 — B6 still measures unbounded saturation against a near-zero mock

File: `scripts/bench_real.py`.

Current B6:

```python
_, _, rps, non200 = oha_run(ROUTER, "1k", "200", sat_out, "sat")
```

This is still an unbounded closed-loop saturation run. Against a ~0 ms mock, it can drive the router into ledger overflow/log/backpressure territory and produce empty/invalid `oha` JSON. That is not a clean SOTA router-minus-direct measurement; it mixes router overhead with an artificial infinite-source overload condition.

Fix B6 as a **target-rate sustained throughput gate**:

- Add `B6_TARGET_RPS = int(os.environ.get("B6_TARGET_RPS", str(B6_MIN_RPS)))`.
- Extend `oha_run(..., rate=None)` so both warmup and measured run include `-q <rate>` when `rate` is set.
- Run B6 with `conc=200` and `rate=B6_TARGET_RPS`.
- Keep pass condition strict: actual `requestsPerSec >= B6_MIN_RPS` and `non200 == 0`.
- Put `B6_TARGET_RPS` in `summary.json` / `gate.json` knobs.

Concrete patch shape:

```python
B6_TARGET_RPS = int(os.environ.get("B6_TARGET_RPS", str(B6_MIN_RPS)))


def oha_run(url, payload, conc, out, raw_label, rate=None):
    base = ["oha", "-z", DUR, "-c", str(conc), "-m", "POST", "--no-tui", "--latency-correction"]
    if rate is not None:
        base += ["-q", str(rate)]
    # add headers/body/output/url after base
```

For warmup, use the same rate knob:

```python
warm = ["oha", "-z", WARM, "-c", str(conc), "-m", "POST", "--no-tui"]
if rate is not None:
    warm += ["-q", str(rate)]
```

B6 call:

```python
_, _, rps, non200 = oha_run(
    ROUTER,
    "1k",
    "200",
    sat_out,
    "sat",
    rate=B6_TARGET_RPS,
)
```

This keeps B6 meaningful: can the router sustain the published threshold under conc=200 with zero non-200, without turning the benchmark into a log-flood/queue-overflow stress test.

## Root cause 3 — `oha` JSON is trusted before validation

File: `scripts/bench_real.py`.

Do not compute p50/p99/rps from malformed or aborted `oha` output. The previous failed validation produced artifacts like empty `statusCodeDistribution` and `errorDistribution={"aborted due to deadline": 200}`. A SOTA benchmark harness must fail loudly with the raw file path and stderr tail.

Add a parser/validator and use it inside `oha_run` before returning numbers:

```python
def parse_oha_json(out, raw_label, payload, conc, stderr):
    try:
        data = json.load(open(out))
    except Exception as e:
        raise RuntimeError(f"invalid oha JSON {out} ({raw_label} {payload} c={conc}): {e}; stderr={stderr[-2000:]}")

    errors = data.get("errorDistribution") or {}
    statuses = data.get("statusCodeDistribution") or {}
    if errors:
        raise RuntimeError(f"oha transport errors in {out} ({raw_label} {payload} c={conc}): {errors}; stderr={stderr[-2000:]}")
    if not statuses:
        raise RuntimeError(f"oha produced empty statusCodeDistribution in {out} ({raw_label} {payload} c={conc}); stderr={stderr[-2000:]}")

    lp = data.get("latencyPercentiles") or {}
    summary = data.get("summary") or {}
    for k in ("p50", "p99"):
        if not isinstance(lp.get(k), (int, float)):
            raise RuntimeError(f"oha missing numeric latencyPercentiles.{k} in {out}: {lp}")
    if not isinstance(summary.get("requestsPerSec"), (int, float)):
        raise RuntimeError(f"oha missing numeric summary.requestsPerSec in {out}: {summary}")
    return data
```

Then:

```python
data = parse_oha_json(out, raw_label, payload, conc, p.stderr)
non200 = sum(v for k, v in data.get("statusCodeDistribution", {}).items() if k != "200")
```

For B1/B2 router runs, a non-zero `non200` must mark the gate fail as it already does. For B6, `non200` must be zero.

## Root cause 4 — stale `benchmarks/gate.sh` still has Python 3.11-only TOML loading

File: `benchmarks/gate.sh`.

Even if `Makefile` now prefers `scripts/bench_real.py`, this executable benchmark file remains in the repo and can fail on the current Python 3.10 environment because it imports `tomllib` unconditionally.

Either delete/demote this file if it is no longer canonical, or fix the loader. If keeping it, use this exact shape:

```bash
thr() {
  python3 - "$1" <<'PY'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open('thresholds.toml', 'rb') as f:
    t = tomllib.load(f)
print(eval('t' + sys.argv[1], {'__builtins__': {}}, {'t': t}))
PY
}
```

Current environment evidence:

```text
Python 3.10.15
import tomllib -> ModuleNotFoundError
import tomli   -> OK
```

## Keep this part from current patch

File: `src/ledger/mod.rs`.

The current rate-limited ledger drop logging is the correct architectural direction:

```rust
metrics::counter!("router_ledger_dropped_total").increment(1);
// log at most once per second
```

Do not revert to per-drop `tracing::error!`. Per-drop logging under overload is a latency killer and can corrupt B6 evidence by measuring stderr/log pressure instead of router overhead.

## Acceptance commands

Run these on the real worktree, not a temp copy:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 B6_TARGET_RPS=8000 python3 scripts/bench_real.py
```

After the smoke run, inspect the newest `bench/results/<timestamp>/`:

```bash
python3 - <<'PY'
import json, pathlib
root = pathlib.Path('bench/results')
latest = max([p for p in root.iterdir() if p.is_dir()], key=lambda p: p.stat().st_mtime)
print('latest', latest)
gate = json.load(open(latest / 'gate.json'))
summary = json.load(open(latest / 'summary.json'))
print('gate_pass_field', gate.get('pass'))
print('b6', summary.get('b6_saturation'))
raw = sorted(latest.glob('*.json'))
print('raw_json_files', len(raw))
for f in raw:
    if f.name in ('gate.json', 'summary.json'):
        continue
    d = json.load(open(f))
    assert d.get('statusCodeDistribution'), f'{f}: empty statusCodeDistribution'
    assert not (d.get('errorDistribution') or {}), f'{f}: errorDistribution={d.get("errorDistribution")}'
print('raw_oha_valid')
PY
```

Expected smoke result:

- script exits 0 with `REQUIRE_PASS=0`;
- `gate.json` and `summary.json` exist;
- raw direct/router/sat `oha` JSON files exist and validate;
- B6 raw JSON has non-empty `statusCodeDistribution`, empty `errorDistribution`, numeric `requestsPerSec`, and `non200 == 0`;
- router logs do not flood ledger drop errors.

Then run the real gate before any SOTA claim:

```bash
python3 scripts/bench_real.py
```

For SOTA claim, `gate.json.pass` must be `true`, raw `oha` artifacts must be retained, and the report must include hardware/kernel/SHA/knobs. Anything less is smoke evidence only.
