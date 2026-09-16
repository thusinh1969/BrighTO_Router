# CODEX urgent audit — new request scanner regresses correctness and speed

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree after DeepSeek added `src/request.rs` and wired it into `handlers.rs`/`proxy.rs`.
Rule from `audits/README.md`: Codex does not edit `src/`; this is exact repair guidance.

## Verdict

Do not ship the current `request::scan` path as-is.

It improves the architecture direction by trying to avoid full-body serde parsing, but the current implementation has two verified regressions:

1. **Correctness P0:** invalid JSON can return `Some(RequestHead)` instead of `None`, so `handlers.rs` skips serde fallback and can forward malformed JSON to the backend.
2. **Performance P1:** current scanner is slower than the old serde `RequestHead` parse on 1K/50K/200K payloads in a release micro-bench, because it still walks the whole `messages` string byte-by-byte to prove root `stream_options` absence.

Fix this before claiming the request-head scanner is a SOTA improvement.

## Current code shape

`src/handlers.rs` now does:

```rust
let (model, stream, has_stream_options) = match request::scan(&body_bytes) {
    Some(h) => (h.model, h.stream, h.has_stream_options),
    None => {
        let h: RequestHead<'_> = serde_json::from_slice(&body_bytes)?;
        (h.model, h.stream, proxy::contains_stream_options(&body_bytes))
    }
};
```

Therefore `request::scan` must be conservative: if the JSON is malformed or if the scanner cannot prove validity, it must return `None` and let serde reject/fallback.

## Correctness proof

A temp harness calling `brigto_router::request::scan` on current source produced:

```text
valid: scan=Some(("x", true, false)) serde_ok=true
trailing_garbage: scan=Some(("x", false, false)) serde_ok=false
trailing_comma: scan=Some(("x", false, false)) serde_ok=false
double_comma: scan=Some(("x", true, false)) serde_ok=false
bad_unknown_literal: scan=Some(("x", false, false)) serde_ok=false
literal_newline_in_string: scan=Some(("x\n", false, false)) serde_ok=false
```

These cases must return `None` from `scan`, not `Some`, because serde says they are invalid JSON.

Root causes in `src/request.rs`:

- after seeing root `}`, scanner returns success without requiring only whitespace until EOF;
- loop accepts commas whenever it sees them, so trailing/double comma are accepted;
- `skip_scalar` accepts arbitrary bytes until comma/close, so invalid literals like `nul` are accepted;
- `parse_string` does not reject unescaped control chars and does not validate escape sequences.

## Performance proof

Release micro-bench using current `brigto_router::request::scan` vs old `serde_json::from_slice::<RequestHead>`:

```text
scan  1k-model-first-no-stream-options:      3.108 us/iter
serde 1k-model-first-no-stream-options:      1.230 us/iter

scan  50k-model-first-no-stream-options:    95.673 us/iter
serde 50k-model-first-no-stream-options:    33.393 us/iter

scan  200k-model-first-no-stream-options:  380.790 us/iter
serde 200k-model-first-no-stream-options:  133.183 us/iter

scan  200k-model-last:                     446.605 us/iter
serde 200k-model-last:                     128.459 us/iter
```

Reason: to set `has_stream_options=false`, scanner must inspect the whole root object. Current `parse_string`/`skip_container` walk huge prompt strings byte-by-byte in Rust. Serde is currently faster.

## Exact fix options

### Option A — safest immediate fix: rollback scanner from hot path

Use the old serde `RequestHead` parse in `handlers.rs` until a strict scanner beats serde in benchmark.

Keep `src/request.rs` behind tests/bench if useful, but do not call it in request forwarding path while it is both less correct and slower.

This is acceptable because the measured serde parse cost at 200K is ~130us on this machine, below the 2ms p99 overhead target, while the current scanner costs ~380–447us and accepts invalid JSON.

### Option B — keep scanner only after strictness + perf fixes

If keeping the scanner now, it must satisfy these before being used by `handlers.rs`:

1. Add strict invalid JSON tests:

```rust
#[test]
fn invalid_json_returns_none() {
    assert!(scan(br#"{"model":"x"} garbage"#).is_none());
    assert!(scan(br#"{"model":"x",}"#).is_none());
    assert!(scan(br#"{"model":"x",,"stream":true}"#).is_none());
    assert!(scan(br#"{"model":"x","foo":nul}"#).is_none());
    assert!(scan(b"{\"model\":\"x\n\"}").is_none());
}
```

2. Enforce JSON object grammar:

- after `{`, expect either key or `}`;
- after a value, expect either `,` then key, or `}`;
- no leading comma, double comma, or trailing comma;
- after root `}`, skip whitespace and require EOF.

3. Make `parse_string` conservative:

- reject unescaped bytes `< 0x20`;
- validate escapes: `"`, `\\`, `/`, `b`, `f`, `n`, `r`, `t`, and `u` followed by 4 hex digits;
- for escaped `model` or escaped root key, return `None` so serde handles unescape correctly.

4. Make scalar skipping conservative:

- accept only valid `true`, `false`, `null`, or JSON number grammar;
- unknown invalid scalar -> `None`.

5. Add a micro-bench gate before using scanner in hot path:

```text
request::scan 200k no-stream-options must be faster than serde RequestHead parse, not slower.
```

If this does not pass, rollback to serde for request-head extraction.

## Root `stream_options` handling

The old global `proxy::contains_stream_options(&body_bytes)` fallback is still not root-aware. If scanner returns `None` only for escaped but valid JSON, global memmem can still confuse nested `stream_options` with root `stream_options`.

Acceptable temporary behavior:

- rollback to old serde + global memmem if prioritizing stability, but document exact-usage false positive as P1;
- or implement one strict root scanner for root key presence and use it for `model`, `stream`, and `stream_options` only after it is strict and fast enough.

Do not parse into `serde_json::Value` and re-encode the full request. That would be worse for hot-path latency and request fidelity.

## Current priority order

1. Finish admin reload acknowledgement race from `CODEX-admin-reload-race-patch-ready-20260916.md`.
2. Fix or rollback current request scanner before benchmark claim.
3. Re-run full gates and production smoke.
4. Only then run SOTA benchmark matrix.

## Added runtime proof — invalid JSON is forwarded

After rebuilding current `target/release/brigto-router`, I ran a fresh Postgres smoke with a mock backend hit counter and sent this invalid request body:

```text
{"model":"x","messages":[]} trailing-garbage
```

Result:

```text
MOCK_HIT raw_suffix= b'[]} trailing-garbage'
RESULT status 200 hits 1 resp {"ok":true,"usage":{"prompt_tokens":1,"completion_tokens":1}}
```

So this is production-visible: the router accepted malformed JSON, forwarded it upstream, and returned 200 because the mock backend responded 200. The scanner must return `None` for malformed JSON so the existing serde fallback rejects it with 400.
