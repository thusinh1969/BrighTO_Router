# Benchmark gate bug — B6 throughput does not actually force concurrency 200

Time: 2026-09-16 22:xx ICT  
Scope: current benchmark harness. Codex did not edit benchmark scripts.

## Verdict

Current `benchmarks/gate.sh` and copied `benchmarks/router-setup/bench/gate.sh` contain a shell semantics bug in the B6 throughput gate. The script intends to benchmark 1K non-stream throughput at concurrency 200, but `run_oha` still sees the previous `CONC` value, normally 50.

This makes the SOTA/fastest gate untrustworthy. Do not use current B6 output as evidence for “8,000 rps on 4 cores”.

## Source evidence

Current line in both files:

```bash
CONC=200 read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json")
```

`run_oha()` reads global `$CONC` internally:

```bash
oha -z "$WARM" -c "$CONC" ...
oha -z "$DUR"  -c "$CONC" ...
```

## Shell proof

Minimal reproduction:

```bash
bash -lc 'CONC=50; f(){ printf "%s\n" "$CONC"; }; CONC=200 read -r x < <(f); printf "x=%s outer=%s\n" "$x" "$CONC"'
```

Output:

```text
x=50 outer=50
```

Correct behavior if variable is set before process substitution:

```bash
bash -lc 'CONC=50; f(){ printf "%s\n" "$CONC"; }; CONC=200; read -r x < <(f); printf "x=%s outer=%s\n" "$x" "$CONC"'
```

Output:

```text
x=200 outer=200
```

## Exact patch

Prefer making `run_oha` accept concurrency explicitly so future gates cannot inherit the wrong global:

```bash
run_oha() { # $1=url $2=payload $3=out $4=conc(optional)
  local conc="${4:-$CONC}"
  oha -z "$WARM" -c "$conc" -m POST --no-tui \
      -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
      -D "payloads/$2" "$1/v1/chat/completions" >/dev/null 2>&1 || true
  oha -z "$DUR" -c "$conc" -m POST --no-tui --latency-correction \
      -H 'Content-Type: application/json' -H "Authorization: Bearer $ROUTER_KEY" \
      -D "payloads/$2" -o "$3" --output-format json "$1/v1/chat/completions" >/dev/null
  jq -r '[.latencyPercentiles.p50, .latencyPercentiles.p99, .summary.requestsPerSec, ([.statusCodeDistribution|to_entries[]|select(.key!="200")|.value]|add//0)]|@tsv' "$3"
}
```

Then B6 becomes:

```bash
read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
```

Apply this to both:

- `benchmarks/gate.sh`
- `benchmarks/router-setup/bench/gate.sh`

If keeping the copied install harness, also sync the same fix into the copy that will be shipped to users.

## Validation

Before running expensive benchmarks, add a cheap self-check in `gate.sh` or temporarily print the effective conc in `run_oha` for B6:

```text
B6 effective concurrency: 200
```

Then run a short smoke:

```bash
DUR=3s WARM=1s RUNS=1 CONC=50 ROUTER_KEY=<valid> ./benchmarks/gate.sh
```

Inspect `router-sat.json`: it must be produced by a command using `-c 200`, while B1/B2/B3 still use the configured `CONC=50`.

## Non-negotiable

Do not claim SOTA throughput from current benchmark artifacts until this is fixed and a fresh `gate.json` is produced after the fix. A benchmark gate that silently measures the wrong concurrency is worse than no benchmark gate.
