# A1 — config loader (DB -> ArcSwap snapshot)

**Owned:** `src/config/mod.rs` (file duy nhat). Doc `src/contract.rs` (chi doc, khong sua).

## Nhiem vu
- `DbConfigLoader::load_snapshot()`: doc 5 bang (backends, model_routes, teams, api_keys, usage_ledger cho counter luc boot)
  thanh `ConfigSnapshot`. sqlx runtime-tokio + `tls-rustls-aws-lc-rs`. **Dung `sqlx::query_as` runtime, KHONG dung macro `query!`**
  (dev chay SQLite, prod Postgres — macro compile-time mot dialect).
- `api_key_ref` la **ten bien env** (hoac duong file mount) chua key backend — resolve luc load, khong plaintext trong DB.
- Task nen poll moi `poll_secs` (mac dinh 5s) -> build snapshot moi -> `ArcSwap::store` nguyen khoi. Hot path chi `load_full()`.
- Boot: snapshot dau tien PHAI nap xong truoc khi nhan traffic (B1 se await). Poll fail -> giu snapshot cu + log warn (khong sap).

## Test (unit, SQLite in-memory `sqlite::MemoryPool`, khong mock DB)
- `snapshot_picks_up_budget_change_within_poll_interval` — doi DB, cho <= poll_secs, snapshot moi thay gia tri moi.
- `api_key_ref_resolves_from_env` — set env var, load, key backend resolve dung.
- `load_survives_empty_db` — DB trong, snapshot rong nhung hop le.

## Rang buoc
- Khong giu lock guard qua `.await` (DashMap/ArcSwap rule o 00_orchestrator).
- Khong them dependency. Khong sua contract.
