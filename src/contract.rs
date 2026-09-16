//! CONTRACT — kiểu dữ liệu dùng chung + runtime state.
//! Quy tắc hot path: không lock qua .await, không đọc disk/env, không parse full JSON body,
//! không tạo HTTP client mỗi request. ConfigSnapshot đọc qua load_full() (lock-free).
//!
//! DB: sqlx runtime-tokio + aws-lc-rs (một crypto provider duy nhất — xem Cargo.toml).
//! Dùng sqlx::query runtime (KHÔNG macro query!) để build không cần DB; production state là PostgreSQL.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};

use crate::budget::RamBudgetStore;
use crate::ledger::LedgerSink;
use crate::metrics::Metrics;
use crate::route::RamBackendPool;

pub type KeyHash = [u8; 32]; // SHA-256 của API key plaintext

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendFormat {
    OpenAi,
    Anthropic,
}

#[derive(Clone)]
pub struct Backend {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    /// env:NAME hoặc file:/path hoặc tên env — KHÔNG bao giờ plaintext trong DB.
    pub api_key_ref: String,
    /// Key đã resolve lúc load config (runtime-only, không serialize, không in ra log).
    pub api_key: Option<String>,
    pub weight: u32,       // least-load chia cho số này; >= 1
    pub max_inflight: u32, // 0 = không giới hạn
    pub format: BackendFormat,
    pub enabled: bool,
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backend")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key_ref", &self.api_key_ref)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("weight", &self.weight)
            .field("max_inflight", &self.max_inflight)
            .field("format", &self.format)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct ModelRoute {
    pub model_name: String,               // tên client gọi
    pub backend_ids: Vec<i64>,            // theo thứ tự ưu tiên
    pub fallback_backend_id: Option<i64>, // khai báo tường minh, mặc định tắt
    pub chars_per_token: f64,             // chỉ để ước lượng chặn sớm, không dùng để tính tiền
    pub first_byte_timeout: Duration,     // mặc định 180s, config per model
}

#[derive(Debug, Clone)]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub budget: Option<Budget>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub period: String, // "day" | "month"
    pub max_tokens: u64,
    /// Money budget (USD cents). Optional; khi có -> dùng cho dashboard/đơn vị tiền.
    #[serde(default)]
    pub max_usd_cents: Option<u64>,
    #[serde(default)]
    pub per_model: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKey {
    pub id: i64,
    pub key_hash: KeyHash,
    pub key_prefix: String, // 8 ký tự đầu — để nhận diện/thu hồi
    pub team_id: i64,
    pub owner: String,
    pub allowed_models: Vec<String>, // rỗng = theo team / tất cả
    pub budget: Option<Budget>,      // budget riêng, đè budget team nếu có
    pub rpm_limit: Option<u32>,
    pub concurrency_limit: Option<u32>,
    pub expires_at: Option<i64>, // unix epoch giây
    pub enabled: bool,
}

/// Snapshot config đọc từ DB, nạp lúc boot + poll 5s. Bất biến: hot path chỉ load_full(),
/// swap nguyên khối — không sửa tại chỗ.
#[derive(Debug, Default)]
pub struct ConfigSnapshot {
    pub backends: HashMap<i64, Backend>,
    pub routes: HashMap<String, ModelRoute>,
    pub teams: HashMap<i64, Team>,
    pub keys_by_hash: HashMap<KeyHash, ApiKey>,
}

/// Lý do từ chối trước khi forward (rate limit / budget).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    RateLimited { retry_after: Duration },
    BudgetExceeded { remaining: u64 },
}

/// Các scope counter budget mà RamBudgetStore enforce. Dùng để seed từ ledger lúc boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageScope {
    KeyTotal,
    KeyModel,
    TeamTotal,
    TeamModel,
}

/// Một mẩu usage đã dùng trong period hiện tại, đọc từ usage_ledger để seed counter RAM lúc boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSeed {
    pub scope: UsageScope,
    pub id: i64,
    pub model: String, // "" cho total scope
    pub period_start: i64,
    pub used_tokens: u64,
}

/// Sự kiện usage — đúng bộ cột của bảng usage_ledger. Số token lấy từ usage backend trả về;
/// chỉ khi backend không trả mới ước lượng (estimated=true).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub ts: i64,
    pub request_id: String,
    pub key_id: i64,
    pub team_id: i64,
    pub model: String,
    pub backend_id: i64,
    pub status: u16,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated: bool,
    pub ttfb_ms: u64,
    pub total_ms: u64,
    pub router_overhead_ms: u64,
    /// Epoch millis when the request finished in the router. Used by BENCHMARK.md B10.
    pub completed_at_ms: u64,
    pub stream: bool,
    pub client_aborted: bool,
    pub error_class: Option<String>,
}

/// Runtime state dùng chung. Dùng concrete type (không trait object) để wiring không thể bị bỏ sót
/// và hot path không trả giá vtable dispatch. Mọi thành phần đều Send + Sync.
pub struct AppState {
    pub cfg: Arc<ArcSwap<ConfigSnapshot>>,
    pub budget: Arc<RamBudgetStore>,
    pub backends: Arc<RamBackendPool>,
    pub client: reqwest::Client,
    pub ledger: LedgerSink,
    pub metrics: Metrics,
    pub max_body_bytes: usize,
    /// Admin gọi notify_one() sau mutation -> poll task reload ngay (không chờ 5s).
    pub reload_notify: Arc<tokio::sync::Notify>,
    /// epoch ms lần cuối config reload THÀNH CÔNG / THẤT BẠI — cho /readyz (control-plane, không hot path).
    pub config_ok_at: Arc<AtomicU64>,
    pub config_err_at: Arc<AtomicU64>,
    /// /readyz: nếu lần reload thành công cuối quá cũ (reload treo / DB chết) -> 503, dù chưa có err.
    pub readiness_max_stale_ms: u64,
}
