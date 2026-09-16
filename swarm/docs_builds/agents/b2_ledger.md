# B2 — ledger: channel + batch writer + fallback file + replay

**Owned:** `src/ledger/`, `migrations/` (MOI — file migration SQL, dat ten `0001_init.sql`). Doc `src/contract.rs`.

## Nhiem vu
- `migrations/0001_init.sql`: 5 bang cau hinh (backends, model_routes, teams, api_keys — schema khop contract.rs,
  type dung cho SQLite; ghi chu thich Postgres type tuong ung o comment vi prod dung Postgres, dev dung SQLite file)
  + `usage_ledger` append-only (dung 15 cot cua `UsageEvent` trong contract) + index `(team_id, ts)`, `(key_id, ts)`.
  Seed 1 team demo + 1 key demo (hash cua `lc-dev0001`, prefix `lc-dev000`) de smoke test.
- Ledger writer task (`run(rx)`): gom `batch_size` (100) record HOAC `flush_secs` (1) -> batch INSERT (1 câu, multi-row).
  Hot path dung `try_send` — day -> ghi file `LEDGER_FALLBACK_FILE` (JSONL append-only, 1 dong/UsageEvent), KHONG BAO GIO block.
- Replay: khi DB song lai, doc file fallback, INSERT tung dong/lot, xoa dong da replay (atomic rename file tam thoi).
- Counter RAM luc boot (B2 cung cap ham `boot_counter() -> HashMap` cho A2 nap vao; A2 giu counter, B2 chi tra so lieu).
- Test (SQLite file tam thoi trong /tmp, dung `sqlx::sqlite::SqlitePool` truc tiep — A1 dung AnyPool, B2 duoc dung
  sqlite driver truc tiep vi ledger la duong viet, khong can dual-dialect; ghi chu ro ly do):
  - `batch_flush_by_size_va_by_time` — 100 record flush ngay; 2 record flush sau flush_secs.
  - `db_down_writes_file_replay_on_reconnect` — mock DB fail (pool chi vao file khong ton tai -> fail connect),
    ghi file day du, replay du khi DB gia lap song lai.
  - `try_send_never_blocks` — day channel van tra ve ngay.
