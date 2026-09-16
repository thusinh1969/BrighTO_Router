# A2 — auth + budget (RAM, hot path lock-free)

**Owned:** `src/auth.rs`, `src/budget/mod.rs`. Doc `src/contract.rs` (khong sua).

## Nhiem vu
- `hash_key`: SHA-256 (crate `sha2` da co trong Cargo.toml) -> `[u8;32]`.
- `authorize`: tra HashMap trong snapshot (lock-free `load_full`). Key khong ton tai/inactive/het han -> 401 (chan TRUOC khi doc body).
  Dung key sai `allowed_models` -> 403. Interface tra ve kieu du de handler phan biet 401 vs 403 (giu chu ky contract;
  neu can them enum AuthOutcome thi ghi INBOX — orchestrator quyet).
- `RamBudgetStore` (DashMap + AtomicU64):
  - `try_reserve(key, model, est_tokens)`: token bucket rpm per key + budget per key/team/(team,model) theo period
    ngay/thang. `est_tokens` CHI de tu choi som, khong ghi so. Vuot -> `Decision::RateLimited{retry_after}` /
    `BudgetExceeded{remaining}`.
  - `commit(key, model, tokens)`: atomic add, co hieu luc ngay cho request ke tiep.
- Concurrency limit per key: gauge atomic, tang/giam quanh moi request (handler goi; ban cung cap API tang/giam).

## Test (unit, khong mock DB — snapshot dung tay trong test)
- `budget_10k_blocks_at_first_request_over_estimate` — set 10K token, request vuot bi chan, khong vuot qua 1 request uoc luong.
- `rpm_bucket_refills` — bucket refill dung toc do (dung tokio::time::pause duoc thi dung).
- `expired_key_rejected`, `wrong_model_403_vs_wrong_key_401`.

## Rang buoc
- `try_reserve`/`commit` KHONG duoc await (chay trong hot path). DashMap chi lock 1 shard, khong giu guard qua await.
