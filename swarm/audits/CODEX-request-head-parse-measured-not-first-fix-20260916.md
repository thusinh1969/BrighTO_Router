# CODEX request-head parse measurement — do not rewrite first

Date: 2026-09-16 22:14 +07

Verdict: current `serde_json::from_slice::<RequestHead>` is payload-size dependent, but measured cost is about 141.5µs on the current 800KB `200k.json` payload. It is not the first root-cause fix unless `BRIGTO_HOTPATH_PROBE` shows `handler.parse_head` dominates `total_us` after benchmark-truth and response/route patches.

## Evidence

Current handler path:

```rust
let head: RequestHead<'_> = serde_json::from_slice(&body_bytes)?;
```

Payload sizes on this worktree:

```text
1k.json      4,184 bytes
50k.json   200,184 bytes
200k.json  800,184 bytes
```

Measurement method: temp release binary in `/tmp/brigto_parse_measure_qUjpB54x` using the same `RequestHead` shape as current `src/handlers.rs`:

```rust
#[derive(Deserialize)]
struct RequestHead<'a> {
    #[serde(borrow)]
    model: &'a str,
    #[serde(default)]
    stream: bool,
    #[serde(default, borrow)]
    stream_options: Option<&'a RawValue>,
}
```

Command:

```bash
cargo run --release --bin request_head_parse_bench
```

Observed output:

```text
payload=1k bytes=4184 iters=2000 total_ms=2.758 per_us=1.379
payload=50k bytes=200184 iters=2000 total_ms=86.480 per_us=43.240
payload=200k bytes=800184 iters=2000 total_ms=283.054 per_us=141.527
```

## Interpretation

This cost is real, but it is much smaller than the current 200k benchmark miss reported in earlier verified benchmark-truth smoke. Replacing it with a handwritten scanner is risky because previous scanner attempts accepted malformed JSON and were slower on this payload class.

Relevant historical warnings:

- `CODEX-request-scanner-regression-20260916.md`
- `CODEX-urgent-round5-progress-false-green-20260916.md`

## Action for DeepSeek

Do not make a parser rewrite the next patch by default.

Use this order:

1. Fix benchmark truth.
2. Apply no-gzip identity backend request.
3. Apply route no-alloc.
4. Apply non-stream response streaming.
5. Run focused 200k probe.
6. Only if `handler.parse_head` dominates `total_us`, replace serde with a strict top-level scanner.

Acceptance for any future scanner:

- Must reject malformed JSON cases currently rejected by serde, or the behavior change must be explicitly accepted in BENCHMARK/contract docs.
- Must correctly distinguish root `stream_options` from nested/string occurrences.
- Must beat current serde parse on 1k/50k/200k release microbench.
- Must not parse into `serde_json::Value` or re-encode request body.
