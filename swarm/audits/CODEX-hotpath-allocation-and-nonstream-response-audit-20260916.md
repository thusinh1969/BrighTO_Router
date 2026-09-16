# HOT PATH AUDIT — remove avoidable allocations only after benchmark truth lands

Time: 2026-09-16 21:45 +07.  
Scope: current real worktree.  
Codex rule in this repo: audit only; do not edit `src/` directly from Codex.

## Verdict

Do not start with these optimizations before applying the benchmark-truth patch. But once the harness is truthful, the next measured 1k/200k work should inspect these exact hot-path costs. They are concrete source-level costs, not architecture theory.

Current top priority remains:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Then run the short truthful benchmark and only optimize against that artifact.

## P1 — non-stream responses are buffered before client response starts

Source evidence:

```text
src/proxy/mod.rs:475   async fn forward_backend_response(...)
src/proxy/mod.rs:534   } else {
src/proxy/mod.rs:535       match response.bytes().await {
src/proxy/mod.rs:537           let acc = parse_usage_from_body(&body_bytes, format);
src/proxy/mod.rs:539           reporter.finish(...);
src/proxy/mod.rs:547           build_response(..., Body::from(body_bytes))
```

Impact:

```text
For non-stream completions, the router waits for the entire upstream response body, parses usage, writes ledger event, emits metrics, then starts returning the body to the client.
```

This is not acceptable for a SOTA latency router with large non-stream completions. The current benchmark mock probably returns a small body, so this is not the main 200k-upload miss, but it is still an architecture blocker before claiming fastest in production workloads.

Fix shape, without over-engineering:

```text
Unify response forwarding around streaming body forwarding.
For stream=true: keep existing SSE usage accumulator path.
For stream=false: forward upstream chunks to client immediately while accumulating only what is needed for final usage parse.
Finalize budget/ledger/metrics on EOF/drop, not before returning the response body.
Keep a cap for non-stream usage buffer; if exceeded, mark usage estimated instead of buffering unbounded output.
```

Acceptance:

```text
No resp.bytes().await on the normal proxy path.
Client receives response chunks as upstream chunks arrive.
Ledger/budget/concurrency/backend lease still finalize exactly once on EOF/drop.
A targeted integration test proves a delayed multi-chunk non-stream upstream reaches client before upstream EOF.
Truthful benchmark does not regress 1k/50k/200k p50/p99.
```

Do not rewrite the whole proxy. This is a local change inside `forward_backend_response` plus tests.

## P1 — route picker allocates per request

Source evidence:

```text
src/proxy/mod.rs:614   let mut tried: HashSet<i64> = HashSet::new();
src/route/mod.rs:131   acquire() calls self.acquire_excluding(route, &HashSet::new())
src/route/mod.rs:141   let mut excluded = initial_excluded.clone();
src/route/mod.rs:149   fallback creates a fresh ModelRoute with backend_ids: vec![fallback]
src/route/mod.rs:204   let mut candidates: Vec<Candidate> = Vec::new();
src/route/mod.rs:275   let mut tied: Vec<Candidate> = candidates.into_iter().filter(...).collect();
```

Impact:

```text
The common request path pays heap allocations for HashSet/Vec candidate selection even when the route has one backend and no retry occurs.
```

This can matter for the strict 1k p50 gate. Temp truthful RUNS=3 short run showed 1k c=50 p50 median 0.339ms versus 0.3ms threshold. Do not assume this is the top contributor, but it is cheap to measure and cheap to remove.

Fix shape, no new dependency required:

```text
Replace per-request HashSet with a tiny stack-backed tried list.
Use Vec<i64> only if route.backend_ids.len() exceeds a small fixed inline capacity.
Rewrite choose_candidate as one pass:
  - keep best_score and current best Candidate;
  - for equal score, use reservoir random tie-break without collecting tied Vec;
  - skip IDs by scanning the tiny tried slice.
Avoid constructing fallback ModelRoute; add acquire_single_backend(fallback_id) or pass a single-backend slice.
Keep BackendLease RAII unchanged.
```

Acceptance:

```text
No HashSet allocation on normal no-retry route acquire.
No candidates Vec allocation on normal route acquire.
No tied Vec allocation for random tie-break.
Fallback behavior remains primary-first then explicit fallback_backend_id.
Existing route tests still pass; add one tie-break/fallback regression if current tests do not cover it.
Truthful benchmark after change shows no regression; if 1k p50 improves, keep it.
```

## P1 — metrics labels allocate on response finalization

Source evidence:

```text
src/metrics.rs:37-45    request_total converts team/key/model/backend/status to String each call
src/metrics.rs:49-67    tokens_total converts six labels to String each call
src/metrics.rs:70-87    ttfb/overhead convert labels to String each call
src/proxy/mod.rs:361    ledger.try_record(event)
src/proxy/mod.rs:363-404 metrics emitted inside CompletionReporter::finish
```

Impact:

```text
For non-stream today, these allocations happen before the response is returned to the client because the non-stream path buffers and finalizes before Body::from(body_bytes). For stream=true they happen after stream EOF inside the spawned task.
```

Fix order:

```text
First fix non-stream response forwarding so finalization is no longer before response start.
Then measure whether dynamic label allocation remains visible.
Only if visible, cache common label values in loaded snapshot/runtime metadata; do not remove production metrics.
```

Do not hand-render Prometheus metrics as a shortcut. Keep `metrics` crate unless a measurement proves it dominates.

## Required measurement before patching these

Because benchmark truth is still not applied in real tree, do not claim these explain the full miss yet. Add temporary timers around:

```text
route acquire
backend lookup/clone
build reqwest request
non-stream upstream body wait
usage parse
ledger enqueue + metrics emit
```

Run:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Use only the run produced after the benchmark-truth patch. The old real-worktree benchmark artifacts are invalid for this decision because they read oha seconds as milliseconds and use unbounded pressure.
