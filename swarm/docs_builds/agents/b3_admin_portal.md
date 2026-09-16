# B3 — Admin API + portal 1 trang HTML tinh

**Owned:** `src/admin/mod.rs`, `static/` (MOI). Doc `src/contract.rs` + mockup `docs/llm-router-portal-mockup.html`
(theo mockup do, don gian hoa, KHONG build step, vanilla JS fetch).

## Nhiem vu
- Router con nest vao router chinh (ham `router()` tra `axum::Router` — giu chu ky contract):
  - `POST /admin/teams`, `POST /admin/keys` (tao key: sinh plaintext `lc-` + 32 hex, tra plaintext DUNG 1 LAN,
    luu SHA-256 hash + prefix 8 ky tu), `PATCH /admin/teams/:id` (budget/limits), `DELETE /admin/keys/:id` (disable),
    `GET /admin/usage?team=&from=&to=` (doc tu ledger, ket hop counter RAM qua callback — B2 se cap).
  - Auth admin: master key tu env `ADMIN_MASTER_KEY` + IP allowlist `ADMIN_ALLOW_CIDR` (CSV CIDR, mac dinh gom 127.0.0.1/32).
    Sai key -> 401, IP ngoai allowlist -> 403.
  - Ghi DB qua sqlx (duong admin KHONG hot path -> dung `sqlx::query` voi `sqlx::any` OK nhu A1; dung dung type).
    Sau khi ghi -> bump mot flag/epoch de config loader reload ngay (khong doi 5s — ghi chu INBOX cho B1 wire).
- `static/index.html`: 1 trang tinh, fetch admin API: tao key (hien plaintext 1 lan), set budget, bang usage 30 ngay.
  Theo mockup nhung cat bo phan decor thua. Khong React, khong build step.
- Test: `tests/admin_test.rs` dung tower oneshot + SQLite file tam: tao key -> plaintext tra 1 lan -> hash luu DB ->
  authorize (A2) dung duoc; PATCH budget co hieu luc; IP allowlist chan dung.
