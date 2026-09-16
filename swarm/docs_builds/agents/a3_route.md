# A3 — route + circuit + health (least-load)

**Owned:** `src/route/mod.rs`. Doc `src/contract.rs` (khong sua).

## Nhiem vu
- `pick(route)`: loc backend khoe (chua mo circuit, chua cham `max_inflight`, enabled), chon **it request dang chay nhat
  chia cho trong so** (`inflight/weight`, hoa -> random). Khong backend khoe -> `RouteDecision::None` (handler tra 503 kem ly do).
- `note_result(backend_id, ok)`: circuit passive — **3 loi lien tiep (ket noi/5xx) -> mo 30s -> half-open cho dung 1 request thu**
  -> OK thi dong, fail thi mo lai.
- Active health: task nen `GET /health` (llama-server/vLLM) hoac `GET /v1/models` moi 5s/backend, cap nhat circuit state.
  Ham health-check nen nhan `reqwest::Client` qua constructor de test duoc.
- `inflight(backend_id)`: so request dang chay (atomic counter, tang/giam quanh moi request — handler goi).

## Test (unit, khong can backend that — dung ModelRoute/Backend tay)
- `least_load_picks_idle_backend` — 2 backend, 1 dang 5 inflight -> chon cai 0.
- `circuit_opens_after_3_failures_and_half_opens_after_30s` (dung tokio::time::pause).
- `max_inflight_skips_backend`, `no_healthy_backend_returns_none`.

## Rang buoc
- `pick`/`note_result`/`inflight` la ham sync, khong await. Random dung crate `rand` da co (API 0.10: `rand::rng()`, `.random_range(..)`).
