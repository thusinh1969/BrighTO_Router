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

/// Route-level provider protocol (CODEX provider-protocol taxonomy). Xác định endpoint client
/// gọi + shape upstream, tách khỏi auth_mode. Không overload BackendFormat thành protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProtocol {
    OpenAiChat,
    OpenAiCompletions,
    OpenAiEmbeddings,
    OpenAiRerank,
    CohereRerank,
    VoyageRerank,
    JinaRerank,
    OpenAiAudioTranscriptions,
    AnthropicMessages,
    LocalOpenAiChat,
    CustomOpenAiChat,
}

impl ProviderProtocol {
    /// Parse giá trị lưu trong DB; unknown/empty -> OpenAiChat (backward-compat).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "openai_completions" | "completions" => ProviderProtocol::OpenAiCompletions,
            "openai_embeddings" | "embeddings" => ProviderProtocol::OpenAiEmbeddings,
            "openai_rerank" | "rerank" | "custom_rerank" => ProviderProtocol::OpenAiRerank,
            "cohere_rerank" => ProviderProtocol::CohereRerank,
            "voyage_rerank" => ProviderProtocol::VoyageRerank,
            "jina_rerank" => ProviderProtocol::JinaRerank,
            "openai_audio_transcriptions" | "openai_asr" | "asr" | "transcriptions" => {
                ProviderProtocol::OpenAiAudioTranscriptions
            }
            "anthropic_messages" | "messages" => ProviderProtocol::AnthropicMessages,
            "local_openai_chat" => ProviderProtocol::LocalOpenAiChat,
            "custom_openai_chat" => ProviderProtocol::CustomOpenAiChat,
            _ => ProviderProtocol::OpenAiChat,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ProviderProtocol::OpenAiChat => "openai_chat",
            ProviderProtocol::OpenAiCompletions => "openai_completions",
            ProviderProtocol::OpenAiEmbeddings => "openai_embeddings",
            ProviderProtocol::OpenAiRerank => "openai_rerank",
            ProviderProtocol::CohereRerank => "cohere_rerank",
            ProviderProtocol::VoyageRerank => "voyage_rerank",
            ProviderProtocol::JinaRerank => "jina_rerank",
            ProviderProtocol::OpenAiAudioTranscriptions => "openai_audio_transcriptions",
            ProviderProtocol::AnthropicMessages => "anthropic_messages",
            ProviderProtocol::LocalOpenAiChat => "local_openai_chat",
            ProviderProtocol::CustomOpenAiChat => "custom_openai_chat",
        }
    }

    /// Endpoint client phải gọi để route này chấp nhận (protocol endpoint guard).
    pub fn incoming_path(self) -> &'static str {
        match self {
            ProviderProtocol::OpenAiChat
            | ProviderProtocol::LocalOpenAiChat
            | ProviderProtocol::CustomOpenAiChat => "/v1/chat/completions",
            ProviderProtocol::OpenAiCompletions => "/v1/completions",
            ProviderProtocol::OpenAiEmbeddings => "/v1/embeddings",
            ProviderProtocol::OpenAiRerank
            | ProviderProtocol::CohereRerank
            | ProviderProtocol::VoyageRerank
            | ProviderProtocol::JinaRerank => "/v1/rerank",
            ProviderProtocol::OpenAiAudioTranscriptions => "/v1/audio/transcriptions",
            ProviderProtocol::AnthropicMessages => "/v1/messages",
        }
    }

    /// Nhãn hiển thị cho wizard + lỗi endpoint guard.
    pub fn label(self) -> &'static str {
        match self {
            ProviderProtocol::OpenAiChat => "OpenAI Chat Completions",
            ProviderProtocol::OpenAiCompletions => "OpenAI Completions",
            ProviderProtocol::OpenAiEmbeddings => "OpenAI Embeddings",
            ProviderProtocol::OpenAiRerank => "OpenAI-compatible Rerank",
            ProviderProtocol::CohereRerank => "Cohere Rerank",
            ProviderProtocol::VoyageRerank => "Voyage Rerank",
            ProviderProtocol::JinaRerank => "Jina Rerank",
            ProviderProtocol::OpenAiAudioTranscriptions => "OpenAI-compatible Audio Transcriptions",
            ProviderProtocol::AnthropicMessages => "Anthropic Messages",
            ProviderProtocol::LocalOpenAiChat => "Local OpenAI-compatible Chat",
            ProviderProtocol::CustomOpenAiChat => "Custom OpenAI-compatible",
        }
    }
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
    pub model_name: String,               // tên client gọi (public)
    pub backend_ids: Vec<i64>,            // theo thứ tự ưu tiên
    pub fallback_backend_id: Option<i64>, // khai báo tường minh, mặc định tắt
    pub chars_per_token: f64,             // chỉ để ước lượng chặn sớm, không dùng để tính tiền
    pub first_byte_timeout: Duration,     // mặc định 180s, config per model
    /// Tên model thật ở provider (vd "qwen3.8-flash-next"); "" = dùng public model_name.
    pub provider_model_name: String,
    pub context_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub price_input_per_mtok_usd: Option<f64>,
    pub price_output_per_mtok_usd: Option<f64>,
    pub enabled: bool,
    /// Route-level credential: ref (file:/... | env:...) — NULL/empty chỉ khi auth_mode = none.
    pub provider_key_ref: Option<String>,
    /// bearer | anthropic | none.
    pub auth_mode: String,
    /// Route-level provider protocol (xem ProviderProtocol). Mặc định "openai_chat".
    pub protocol: String,
    /// Key đã resolve lúc load (runtime-only). None khi auth_mode = none.
    pub provider_key: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::ProviderProtocol;

    #[test]
    fn protocol_parse_and_endpoint_mapping() {
        assert_eq!(
            ProviderProtocol::parse("openai_chat"),
            ProviderProtocol::OpenAiChat
        );
        assert_eq!(
            ProviderProtocol::parse("OPENAI_COMPLETIONS"),
            ProviderProtocol::OpenAiCompletions
        );
        assert_eq!(
            ProviderProtocol::parse("openai_embeddings"),
            ProviderProtocol::OpenAiEmbeddings
        );
        assert_eq!(
            ProviderProtocol::parse("cohere_rerank"),
            ProviderProtocol::CohereRerank
        );
        assert_eq!(
            ProviderProtocol::parse("asr"),
            ProviderProtocol::OpenAiAudioTranscriptions
        );
        assert_eq!(
            ProviderProtocol::parse("anthropic_messages"),
            ProviderProtocol::AnthropicMessages
        );
        assert_eq!(
            ProviderProtocol::parse("local_openai_chat"),
            ProviderProtocol::LocalOpenAiChat
        );
        // unknown/empty -> openai_chat (backward-compat)
        assert_eq!(ProviderProtocol::parse(""), ProviderProtocol::OpenAiChat);
        assert_eq!(
            ProviderProtocol::parse("garbage"),
            ProviderProtocol::OpenAiChat
        );
    }

    #[test]
    fn protocol_incoming_path_and_label() {
        assert_eq!(
            ProviderProtocol::OpenAiChat.incoming_path(),
            "/v1/chat/completions"
        );
        assert_eq!(
            ProviderProtocol::LocalOpenAiChat.incoming_path(),
            "/v1/chat/completions"
        );
        assert_eq!(
            ProviderProtocol::OpenAiCompletions.incoming_path(),
            "/v1/completions"
        );
        assert_eq!(
            ProviderProtocol::OpenAiEmbeddings.incoming_path(),
            "/v1/embeddings"
        );
        assert_eq!(ProviderProtocol::OpenAiRerank.incoming_path(), "/v1/rerank");
        assert_eq!(ProviderProtocol::CohereRerank.incoming_path(), "/v1/rerank");
        assert_eq!(
            ProviderProtocol::OpenAiAudioTranscriptions.incoming_path(),
            "/v1/audio/transcriptions"
        );
        assert_eq!(
            ProviderProtocol::AnthropicMessages.incoming_path(),
            "/v1/messages"
        );
        assert_eq!(
            ProviderProtocol::OpenAiChat.label(),
            "OpenAI Chat Completions"
        );
        assert_eq!(
            ProviderProtocol::AnthropicMessages.label(),
            "Anthropic Messages"
        );
    }
}
