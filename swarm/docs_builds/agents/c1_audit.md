# C1 — AUDIT toan cuc (doc tat ca, KHONG sua code)

**Owned:** `swarm/out/c1_audit.md` (report). Doc: toan bo `src/`, `migrations/`, `docs/agents/*`, plan goc
`docs/llm-router-rust-plan.md`.

## Nhiem vu — audit co bang chung, mo ta cach phat hien, khong bao cao cam tinh
1. **Hot path (plan §0):** quet moi `await`/`lock`/`clone` lon tren duong request. Ke ten tung dong vi pham.
2. **Contract:** module nao lech chu ky `contract.rs`; ai sua file khong thuoc quyen (so voi docs/agents/00).
3. **Race:** DashMap guard giu qua await; ArcSwap load_full giu qua await; counter commit vs try_reserve race window
   (2 request cung luc qua try_reserve roi commit — co duoc khong? vi sao).
4. **Panic path:** unwrap/expect/todo! con sot o dau tren hot path; index ngoai pham mang; overflow cong don token.
5. **Header leak:** `x-api-key` client co bi forward len backend khong; backend key co bi log ra khong.
6. **Ledger:** duong ghi file fallback co that su khong block? Replay co mat duong dong nao (crash giua chừng)?
7. **Security:** key hash dung SHA-256? prefix 8 ky tu du de nhan dien ma khong doan duoc? IP allowlist co bi spoof
   qua X-Forwarded-For khong (ke rang TLS terminate tai nginx — XFF chi tin khi tu nginx tin cay)?
8. **Dung plan:** 4 quyet dinh §11 (budget mem RAM, router tu chen stream_options, fallback khai bao, TLS nginx)
   — code co lech?
Moi phat hien: severity (P0 chan san/P1 fix truoc khi ban/P2 ghi chu), file:dong, cach phat hien, de xuat fix.
KHONG sua code — chi report. P0/P2 phai co test/chung chi (lenh + output), khong doan.
