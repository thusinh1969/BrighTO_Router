# Current verification — Anthropic header passthrough P1 is fixed

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

The Anthropic `anthropic-version` overwrite bug is fixed in current source and runtime verified.

## Source evidence

Current `src/proxy/mod.rs` now preserves a client-provided `anthropic-version` and only inserts the default when missing:

```text
src/proxy/mod.rs:103 // Chỉ đặt default nếu client KHÔNG tự gửi anthropic-version...
src/proxy/mod.rs:104 if !req_headers.contains_key("anthropic-version") {
src/proxy/mod.rs:105     req_headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
```

## Runtime proof

Command:

```bash
cargo build --release --locked
python3 /tmp/brigto_anthropic_header_smoke.py
```

Result:

```text
RESULT status 200
CAPTURE anthropic-version 2099-01-01
CAPTURE anthropic-beta test-beta
CAPTURE x-api-key anthropic-backend-secret
CAPTURE authorization None
PASS
ANTHROPIC_HEADER_SMOKE_RC=0
```

This verifies all required properties:

1. client `Authorization` is stripped;
2. backend `x-api-key` is inserted;
3. client `anthropic-version` is preserved;
4. `anthropic-beta` passthrough still works.

## Regression gates after proxy change

```text
cargo fmt --all -- --check                         PASS
cargo clippy --all-targets -- -D warnings          PASS
cargo build --release --locked                     PASS
stream_options runtime smoke                       PASS
```

## Remaining open items

1. `ADMIN_MASTER_KEY` still uses `unwrap_or_default()` unless source changes after this audit.
2. Benchmark B6 shell bug still remains unless source changes after this audit.
3. Full SOTA benchmark matrix remains unproven.
