use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use arc_swap::ArcSwap;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgRow};

use crate::contract::{
    ApiKey, Backend, BackendFormat, Budget, ConfigSnapshot, KeyHash, ModelEndpoint, ModelRoute,
    ProviderProtocol, RoutingPolicy, Team,
};

pub struct DbConfigLoader {
    pub pool: PgPool,
    pub poll_secs: u64,
}

impl DbConfigLoader {
    pub fn new(pool: PgPool, poll_secs: u64) -> Self {
        Self { pool, poll_secs }
    }

    /// Load toàn bộ 4 bảng cấu hình. Hot path không gọi — chỉ task nền gọi.
    pub async fn load_snapshot(&self) -> Result<ConfigSnapshot> {
        let mut snapshot = ConfigSnapshot::default();

        for b in self.load_backends().await? {
            snapshot.backends.insert(b.id, b);
        }
        for r in self.load_routes().await? {
            snapshot.routes.insert(r.model_name.clone(), r);
        }
        for t in self.load_teams().await? {
            snapshot.teams.insert(t.id, t);
        }
        for k in self.load_api_keys().await? {
            snapshot.keys_by_hash.insert(k.key_hash, k);
        }

        Ok(snapshot)
    }

    /// Log usage-ledger totals một lần lúc boot. KHÔNG gọi trong poll 5s —
    /// query SUM toàn bảng ledger sẽ scan bảng usage lớn mỗi poll nếu để chung
    /// với load_snapshot. CODEX: giữ config reload chỉ chạm bảng cấu hình.
    pub async fn log_usage_boot_counter(&self) -> Result<()> {
        let ledger_row = sqlx::query(
            "SELECT COUNT(*), \
             CAST(COALESCE(SUM(input_tokens), 0) AS BIGINT), \
             CAST(COALESCE(SUM(output_tokens), 0) AS BIGINT) FROM usage_ledger",
        )
        .fetch_one(&self.pool)
        .await
        .context("load usage_ledger boot counter")?;
        let cnt: i64 = ledger_row.try_get(0)?;
        let sum_in: i64 = ledger_row.try_get(1)?;
        let sum_out: i64 = ledger_row.try_get(2)?;
        tracing::info!(
            rows = cnt,
            input = sum_in,
            output = sum_out,
            "usage_ledger boot counter"
        );
        Ok(())
    }

    async fn load_backends(&self) -> Result<Vec<Backend>> {
        let rows = fetch_rows(
            &self.pool,
            "SELECT id, name, base_url, api_key_ref, weight, max_inflight, format, enabled FROM backends",
        )
        .await
        .context("load backends")?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id = g_i64(&row, 0)?;
            let name = g_str(&row, 1)?;
            let base_url = g_str(&row, 2)?;
            let api_key_ref = g_str(&row, 3)?;
            let weight = g_i64(&row, 4)?;
            let max_inflight = g_i64(&row, 5)?;
            let format = g_str(&row, 6)?;
            let enabled = g_bool(&row, 7)?;
            let backend_format = match format.as_str() {
                "openai" => BackendFormat::OpenAi,
                "anthropic" => BackendFormat::Anthropic,
                other => return Err(anyhow!("unknown backend format '{other}' for backend {id}")),
            };
            let api_key = resolve_backend_key(&api_key_ref);
            if api_key.is_none() {
                eprintln!("WARN config: cannot resolve api_key_ref {api_key_ref} for backend {id}");
            }
            out.push(Backend {
                id,
                name,
                base_url,
                api_key_ref,
                api_key,
                weight: u32::try_from(weight).unwrap_or(1),
                max_inflight: u32::try_from(max_inflight).unwrap_or(0),
                format: backend_format,
                enabled,
            });
        }
        Ok(out)
    }

    async fn load_routes(&self) -> Result<Vec<ModelRoute>> {
        let mut endpoint_overrides = self.load_route_endpoints().await?;
        let rows = fetch_rows(
            &self.pool,
            "SELECT model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout, \
             provider_model_name, context_tokens, max_output_tokens, \
             price_input_per_mtok_usd, price_output_per_mtok_usd, enabled, \
             provider_key_ref, auth_mode, protocol, routing_policy FROM model_routes",
        )
        .await
        .context("load model_routes")?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let model_name = g_str(&row, 0)?;
            let backend_ids_json = g_str(&row, 1)?;
            let fallback_backend_id = g_opt_i64(&row, 2)?;
            let chars_per_token = g_f64(&row, 3)?;
            let fb_timeout = g_i64(&row, 4)?;
            let backend_ids: Vec<i64> =
                serde_json::from_str(&backend_ids_json).context("parse backend_ids JSON array")?;
            let first_byte_timeout = Duration::from_secs(u64::try_from(fb_timeout).unwrap_or(180));
            let provider_model_name = g_str(&row, 5)?;
            let provider_model_name = if provider_model_name.is_empty() {
                model_name.clone()
            } else {
                provider_model_name
            };
            let provider_key_ref: Option<String> =
                row.try_get::<Option<String>, _>(11).unwrap_or(None);
            let auth_mode = g_str(&row, 12)?;
            let protocol_raw = g_str(&row, 13)?;
            let protocol = ProviderProtocol::parse(&protocol_raw).as_str().to_string();
            let routing_policy = RoutingPolicy::parse(&g_str(&row, 14)?);
            // Resolve route-level credential (nếu có). Khi route không có credential riêng, để
            // provider_key = None; proxy sẽ dùng backend.api_key của backend được chọn tại thời điểm
            // forward (mỗi backend có key riêng, kể cả fallback/secondary).
            let provider_key = if auth_mode == "none" {
                None
            } else if let Some(kr) = &provider_key_ref {
                resolve_backend_key(kr)
            } else {
                None
            };
            let endpoints = endpoint_overrides.remove(&model_name).unwrap_or_default();
            out.push(ModelRoute {
                model_name,
                backend_ids,
                fallback_backend_id,
                chars_per_token,
                first_byte_timeout,
                provider_model_name,
                context_tokens: g_opt_i64(&row, 6)?,
                max_output_tokens: g_opt_i64(&row, 7)?,
                price_input_per_mtok_usd: row.try_get::<Option<f64>, _>(8).unwrap_or(None),
                price_output_per_mtok_usd: row.try_get::<Option<f64>, _>(9).unwrap_or(None),
                enabled: g_bool(&row, 10)?,
                provider_key_ref,
                auth_mode,
                protocol,
                provider_key,
                routing_policy,
                endpoints,
            });
        }
        Ok(out)
    }

    async fn load_route_endpoints(&self) -> Result<HashMap<String, HashMap<i64, ModelEndpoint>>> {
        let rows = fetch_rows(
            &self.pool,
            "SELECT model_name, backend_id, provider_model_name, provider_key_ref, auth_mode, \
             protocol, weight, max_inflight, enabled FROM model_route_endpoints",
        )
        .await
        .context("load model_route_endpoints")?;

        let mut out: HashMap<String, HashMap<i64, ModelEndpoint>> = HashMap::new();
        for row in rows {
            let model_name = g_str(&row, 0)?;
            let backend_id = g_i64(&row, 1)?;
            let provider_model_name = g_str(&row, 2)?;
            let provider_key_ref: Option<String> =
                row.try_get::<Option<String>, _>(3).unwrap_or(None);
            let auth_mode = g_str(&row, 4)?;
            let protocol = ProviderProtocol::parse(&g_str(&row, 5)?)
                .as_str()
                .to_string();
            let weight = g_i64(&row, 6)?.max(1);
            let max_inflight = g_i64(&row, 7)?.max(0);
            let enabled = g_bool(&row, 8)?;
            let provider_key = if auth_mode == "none" {
                None
            } else if let Some(kr) = &provider_key_ref {
                resolve_backend_key(kr)
            } else {
                None
            };
            out.entry(model_name).or_default().insert(
                backend_id,
                ModelEndpoint {
                    backend_id,
                    provider_model_name,
                    provider_key_ref,
                    auth_mode,
                    protocol,
                    weight: u32::try_from(weight).unwrap_or(1),
                    max_inflight: u32::try_from(max_inflight).unwrap_or(0),
                    enabled,
                    provider_key,
                },
            );
        }
        Ok(out)
    }

    async fn load_teams(&self) -> Result<Vec<Team>> {
        let rows = fetch_rows(&self.pool, "SELECT id, name, budget, enabled FROM teams")
            .await
            .context("load teams")?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id = g_i64(&row, 0)?;
            let name = g_str(&row, 1)?;
            let budget_json = g_opt_str(&row, 2)?;
            let enabled = g_bool(&row, 3)?;
            let budget = match budget_json {
                Some(s) => {
                    Some(serde_json::from_str::<Budget>(&s).context("parse team budget JSON")?)
                }
                None => None,
            };
            out.push(Team {
                id,
                name,
                budget,
                enabled,
            });
        }
        Ok(out)
    }

    async fn load_api_keys(&self) -> Result<Vec<ApiKey>> {
        let rows = fetch_rows(
            &self.pool,
            "SELECT id, key_hash, key_prefix, team_id, owner, allowed_models, budget, rpm_limit, concurrency_limit, expires_at, enabled FROM api_keys",
        )
        .await
        .context("load api_keys")?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id = g_i64(&row, 0)?;
            let key_hash_hex = g_str(&row, 1)?;
            let key_prefix = g_str(&row, 2)?;
            let team_id = g_i64(&row, 3)?;
            let owner = g_str(&row, 4)?;
            let allowed_models_json = g_opt_str(&row, 5)?;
            let budget_json = g_opt_str(&row, 6)?;
            let rpm = g_opt_i64(&row, 7)?;
            let conc = g_opt_i64(&row, 8)?;
            let expires = g_opt_i64(&row, 9)?;
            let enabled = g_bool(&row, 10)?;
            let key_hash = hex_to_key_hash(&key_hash_hex).context("parse key_hash hex")?;
            let allowed_models = match allowed_models_json {
                Some(s) => serde_json::from_str::<Vec<String>>(&s).unwrap_or_default(),
                None => Vec::new(),
            };
            let budget = match budget_json {
                Some(s) => {
                    Some(serde_json::from_str::<Budget>(&s).context("parse api key budget JSON")?)
                }
                None => None,
            };
            out.push(ApiKey {
                id,
                key_hash,
                key_prefix,
                team_id,
                owner,
                allowed_models,
                budget,
                rpm_limit: rpm.and_then(|v| u32::try_from(v).ok()),
                concurrency_limit: conc.and_then(|v| u32::try_from(v).ok()),
                expires_at: expires,
                enabled,
            });
        }
        Ok(out)
    }

    /// Poll liên tục: load snapshot → swap vào ArcSwap. Lỗi thì giữ snapshot cũ, log cảnh báo.
    pub async fn run(self, cfg: Arc<ArcSwap<ConfigSnapshot>>) {
        loop {
            match self.load_snapshot().await {
                Ok(snap) => {
                    cfg.store(Arc::new(snap));
                }
                Err(e) => {
                    eprintln!("WARN config: reload failed: {e:#}");
                }
            }
            tokio::time::sleep(Duration::from_secs(self.poll_secs)).await;
        }
    }
}

async fn fetch_rows(pool: &PgPool, sql: &'static str) -> anyhow::Result<Vec<PgRow>> {
    Ok(sqlx::query(sql).fetch_all(pool).await?)
}
fn g_i64(row: &PgRow, idx: usize) -> anyhow::Result<i64> {
    Ok(row.try_get::<i64, _>(idx)?)
}
fn g_opt_i64(row: &PgRow, idx: usize) -> anyhow::Result<Option<i64>> {
    Ok(row.try_get::<Option<i64>, _>(idx)?)
}
fn g_f64(row: &PgRow, idx: usize) -> anyhow::Result<f64> {
    Ok(row.try_get::<f64, _>(idx)?)
}
fn g_str(row: &PgRow, idx: usize) -> anyhow::Result<String> {
    Ok(row.try_get::<String, _>(idx)?)
}
fn g_opt_str(row: &PgRow, idx: usize) -> anyhow::Result<Option<String>> {
    Ok(row.try_get::<Option<String>, _>(idx)?)
}
fn g_bool(row: &PgRow, idx: usize) -> anyhow::Result<bool> {
    Ok(row.try_get::<bool, _>(idx)?)
}

/// Resolve một api_key_ref thành key plaintext. Hỗ trợ env:NAME, file:/path, tên env raw
/// (fallback: path heuristic nếu chứa '/' hoặc đuôi .key). Gọi ở load config (bootstrap + poll),
/// KHÔNG gọi trong hot path — key đã được resolve sẵn vào Backend.api_key.
fn resolve_env_key_with_alias(env_name: &str) -> Option<String> {
    std::env::var(env_name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| match env_name {
            "QWEN_API_KEY" => std::env::var("DASHSCOPE_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            "DASHSCOPE_API_KEY" => std::env::var("QWEN_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            _ => None,
        })
}

pub fn resolve_backend_key(api_key_ref: &str) -> Option<String> {
    if let Some(env_name) = api_key_ref.strip_prefix("env:") {
        return resolve_env_key_with_alias(env_name);
    }
    if let Some(file_path) = api_key_ref.strip_prefix("file:") {
        return std::fs::read_to_string(file_path)
            .ok()
            .map(|s| s.trim().to_string());
    }
    resolve_env_key_with_alias(api_key_ref).or_else(|| {
        if api_key_ref.contains('/') || api_key_ref.ends_with(".key") {
            std::fs::read_to_string(api_key_ref)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    })
}

/// Chuyển chuỗi hex (64 ký tự) thành [u8; 32].
fn hex_to_key_hash(hex: &str) -> Result<KeyHash> {
    if hex.len() != 64 {
        return Err(anyhow!("key_hash hex must be 64 chars, got {}", hex.len()));
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_val(bytes[2 * i]).ok_or_else(|| anyhow!("invalid hex char"))?;
        let lo = hex_val(bytes[2 * i + 1]).ok_or_else(|| anyhow!("invalid hex char"))?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn snapshot_picks_up_budget_change_within_poll_interval(pool: PgPool) {
        sqlx::query(
            "INSERT INTO teams (id, name, budget, enabled) \
             VALUES (1, 'team1', '{\"period\":\"day\",\"max_tokens\":100,\"per_model\":{}}', TRUE)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let loader = DbConfigLoader::new(pool.clone(), 1); // poll 1 giây
        let cfg = Arc::new(ArcSwap::from_pointee(ConfigSnapshot::default()));
        let handle = tokio::spawn(loader.run(cfg.clone()));

        tokio::time::sleep(Duration::from_millis(1200)).await;
        let snap1 = cfg.load_full();
        assert_eq!(
            snap1
                .teams
                .get(&1)
                .unwrap()
                .budget
                .as_ref()
                .unwrap()
                .max_tokens,
            100
        );

        sqlx::query(
            "UPDATE teams SET budget = '{\"period\":\"day\",\"max_tokens\":200,\"per_model\":{}}' WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(1200)).await;
        let snap2 = cfg.load_full();
        assert_eq!(
            snap2
                .teams
                .get(&1)
                .unwrap()
                .budget
                .as_ref()
                .unwrap()
                .max_tokens,
            200
        );

        handle.abort();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn load_survives_empty_db(pool: PgPool) {
        let loader = DbConfigLoader::new(pool, 5);
        let snap = loader.load_snapshot().await.expect("load empty db");
        assert!(snap.backends.is_empty());
        assert!(snap.routes.is_empty());
        assert!(snap.teams.is_empty());
        assert!(snap.keys_by_hash.is_empty());
    }

    #[test]
    fn api_key_ref_resolves_from_env() {
        unsafe {
            std::env::set_var("A1_TEST_BACKEND_KEY", "secret");
        } // Rust 2024: unsafe; test nay don thread
        assert!(resolve_backend_key("A1_TEST_BACKEND_KEY").is_some());
        assert!(resolve_backend_key("env:A1_TEST_BACKEND_KEY").is_some());
        assert!(resolve_backend_key("A1_TEST_NONEXISTENT").is_none());
        unsafe {
            std::env::remove_var("A1_TEST_BACKEND_KEY");
        }
    }
}
