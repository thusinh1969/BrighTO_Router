# Orchestrator — luat choi cua swarm (MOI AGENT DOC TRUOC KHI CODE)

## Quy tac cung
1. **File ownership:** ban CHI duoc sua file cua minh (muc "Owned" trong doc cua ban).
   File chung (`Cargo.toml`, `src/lib.rs`, `src/contract.rs`, `docs/agents/00_*`) chi orchestrator sua.
   Can doi gi ngoai vung -> ghi vao `swarm/out/INBOX.md`, KHONG tu sua.
2. **Contract da chot** (`src/contract.rs`): signature trait/struct khong doi. Implement dung chu ky, thay `todo!()`.
   Muon them method vao trait -> INBOX, orchestrator quyet.
3. **Khong them dependency.** Danh sach Cargo.toml la chot (da khoa version, da verify compile trong Docker).
4. **Comment giai thich TAI SAO, khong giai thich LAM GI.** Code sach, gon, khong over-engineer.
5. **Moi deliverable phai:** `cargo fmt` sach, `cargo clippy --all-targets -- -D warnings` sach
   (`#![allow(unused...)]` o lib.rs chi la tam cho stub — code cua ban khong can no), unit test xanh, test ten mo ta hanh vi.
6. **Hot path khong lock, khong parse full body, khong await DB.** (plan §0 — moi quyet dinh suy tu do.)

## Lenh build/test (moi agent dung lenh nay — code mount tu ngoai vao container Rust)
```bash
cd /home/steve/data02/BrigTO_Router
docker run --rm -v "$PWD":/app -v brigto-cargo-registry:/usr/local/cargo/registry -w /app \
  rust:1.98.1-bookworm bash -c "cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test"
```
Registry cache san trong volume `brigto-cargo-registry` (da compile du ~300 crate) — khong fetch lai.

## Gate cuoi moi wave (orchestrator chay)
merge -> fmt -> clippy -D warnings -> test -> **xanh moi mo wave sau**. Do thi tra ve agent chu file, khong sua ho.

## Wave 1 (song song, 4 slot)
| Slot | Agent | Owned | Doc |
|---|---|---|---|
| 1 | A1 config | `src/config/` | a1_config.md |
| 2 | A2 auth+budget | `src/auth.rs`, `src/budget/` | a2_auth_budget.md |
| 3 | A3 route | `src/route/` | a3_route.md |
| 4 | A4 proxy | `src/proxy/` | a4_proxy.md |

## Wave 2 (sau gate 1, song song 4 slot)
| Slot | Agent | Owned | Doc |
|---|---|---|---|
| 1 | B1 glue | `src/main.rs`, `src/handlers.rs` (moi) | b1_glue.md |
| 2 | B2 ledger | `src/ledger/`, `migrations/` | b2_ledger.md |
| 3 | B3 admin+portal | `src/admin/`, `static/` (moi) | b3_admin_portal.md |
| 4 | C1 audit | doc tat ca, KHONG sua code | c1_audit.md |

## Wave 3 (sau gate 2)
B4 perf harness (`bench/`, `tests/perf_*`) + C2 acceptance (`tests/acceptance/`, llama-server that o 8088)
+ C3 docs (README, ADR, runbook) + C4 hardening (fix list tu C1).

## Boi canh may (da verify bang hanh dong, khong phai gia dinh)
- Backend test that: llama-server `http://127.0.0.1:8088` model `qwen3.8-flash-next`, nhan moi key.
  **Da verify bang request that:** stream thuong KHONG co usage; gui kem `stream_options.include_usage:true`
  -> chunk cuoi CO usage, dang `choices:[]` + field thua `timings` -> parser phai tolerant. Fixture: `tests/fixtures/`.
- Postgres dev: `pgvector/pgvector:pg16` qua docker-compose (port 5432). Valkey `valkey/valkey:9-alpine` (chua dung o v1).
- Build: Rust 1.98.1 trong Docker `rust:1.98.1-bookworm`. Repo root = crate root (`brigto-router`).
- Bay da biet: reqwest 0.13 feature `rustls` = aws-lc-rs; sqlx phai dung `tls-rustls-aws-lc-rs`; KHONG mix ring (panic luc chay).
- `memchr::memmem::find` la API dung de tim chuoi (`memchr` mot byte khong du).
- Sandbox build cua orchestrator chan mot so lenh (vd curl); test HTTP bang python3 stdlib (urllib) hoac reqwest trong Rust test.

## Format handoff (moi agent ghi file `swarm/out/<agent>.md`)
```
STATUS: DONE | BLOCKED
FILES_CHANGED: (chi file cua ban)
TESTS: (ten test + ket qua)
INBOX: (viec can orchestrator quyet / doi file chung)
NOTES: (quyet dinh ky thuat + TAI SAO, se chuyen thanh ADR dang dong)
```
