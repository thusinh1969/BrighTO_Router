# audits/ — nơi GLM audit BrigTO Router

Repo: /home/steve/data02/BrigTO_Router
Snap shot trang thai: 2026-09-16 15:18
Moi file audit ghi vao day (vd audits/GLM-pass1.md). KHONG sua code o src/ — chi ghi ket qua + de xuat vao audits/.

## Cach chay build/test (chuan, dung Docker — code nam ngoai, Rust trong container)
```bash
cd /home/steve/data02/BrigTO_Router
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry \
  -v brigto-rustup:/usr/local/rustup -w /app rust:1.98.1-bookworm bash -c \
  "cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test"
```
Volume cache san co. Test nhanh: `cargo test --lib`.

## Boi canh (da verify bang hanh dong, khong phai gia dinh)
- Backend test that: llama-server http://127.0.0.1:8088 model qwen3.8-flash-next, nhan moi key.
  Stream KHONG co usage neu thieu stream_options; co stream_options -> chunk cuoi co usage, dang
  `choices:[]` + field thua `timings`. Fixtures that: tests/fixtures/.
- Dev DB: SQLite (sqlx `any` driver + `try_get` per-column; KHONG query_as tuple, KHONG u16/u64 — dung i64).
  Prod: Postgres (pgvector/pg16 qua docker-compose). Migration: migrations/0001_init.sql.
- Cong nghe: axum 0.8 (Response<Body> generic), reqwest 0.13 (rustls=aws-lc-rs), sqlx 0.9, rand 0.10
  (RngExt), sha2 0.11, metrics 0.24, dashmap/arc-swap (hot path lock-free).

## Lua audit (GLM can kiem)
1. Hot path (plan §0): khong lock qua await, khong parse full body, khong await DB trong request path.
2. Contract (src/contract.rs): ai sua lech signature; module nao dung trait sai.
3. Race: TOCTOU try_reserve/commit (da phat hien P0 — xem swarm/out/c1_audit.md); DashMap guard qua await.
4. Panic: unwrap/expect/todo!() tren hot path; index out of bounds; overflow token.
5. Header leak: x-api-key client co bi forward; backend key co bi log.
6. Ledger: file fallback co block response khong; replay mat dong giua chung.
7. Security: key hash SHA-256; prefix 8 ky tu; IP allowlist co bi spoof qua X-Forwarded-For.
8. 4 quyet dinh plan §11 (budget mem RAM, router tu chen stream_options, fallback khai bao, TLS nginx).

## Dinh dang bao cao (GLM ghi vao audits/)
Moi phat hien: severity P0/P1/P2 + file:dong + bang chung (lenh + output) + de xuat fix.
KHONG sua code. Neu can chay lenh, chay dung lenh Docker o tren.
