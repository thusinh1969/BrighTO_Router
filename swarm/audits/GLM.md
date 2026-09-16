# GLM audit — hướng dẫn + trạng thái hiện tại

## Trang thai (orchestrator bao cao, 2026-09-16 15:18)
- Wave 1 (config/auth+budget/route/proxy): GATE XANH, commit ccb5595 — 23/23 unit tests pass, clippy clean.
- Wave 2 (handlers/main/ledger/admin+portal): code da merge, GATE CON DO — 25 lib / 26 test errors.
  Tat ca loi da xac dinh dong, dang duoc orchestrator sua mechanical:
  - src/handlers.rs: `use brigto_router::` -> `crate::`; `Response` -> `Response<Body>` (x6).
  - src/admin/mod.rs + src/ledger/mod.rs: sqlx Any phai `query::<sqlx::Any>`, u16/u64 -> i64,
    AnyRow.try_get(idx) (khong .get), clone truoc move, dynamic-SQL allowlist.
- P0 da phat hien boi C1: TOCTOU try_reserve/commit trong trait BudgetStore (contract.rs) — dang xem xet
  them Reservation handle. GLM xac nhan lai + cho de xuat cu the.

## Lua y: GLM dung `audits/` lam noi ghi ket qua, co the tham khao `swarm/out/c1_audit.md`.
