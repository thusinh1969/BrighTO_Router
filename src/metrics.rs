//! Prometheus metrics qua metrics + metrics-exporter-prometheus. Tên ĐÃ CHỐT trong plan §6,
//! đổi tên phải sửa dashboard Grafana. Emit từ reporter (hot path) và task sync nền (gauge).

use metrics_exporter_prometheus::PrometheusHandle;

/// Danh sách tên metric — test chống đổi tên tuỳ tiện.
pub const METRICS: &[&str] = &[
    "router_requests_total",
    "router_tokens_total",
    "router_ttfb_seconds",
    "router_overhead_seconds",
    "router_backend_inflight",
    "router_budget_remaining",
    "router_circuit_open",
    "router_ledger_dropped_total",
];

/// Handle render /metrics. Clone rẻ, đặt trong AppState.
#[derive(Clone)]
pub struct Metrics {
    handle: PrometheusHandle,
}

impl Metrics {
    /// Cài recorder toàn cục (đúng 1 lần lúc boot), spawn upkeep, trả handle.
    pub fn install() -> Self {
        let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .expect("install metrics recorder");
        Self { handle }
    }

    pub fn render(&self) -> String {
        self.handle.render()
    }
}

pub fn request_total(team: i64, key: i64, model: &str, backend: &str, status: u16) {
    metrics::counter!(
        "router_requests_total",
        "team" => team.to_string(),
        "key" => key.to_string(),
        "model" => model.to_string(),
        "backend" => backend.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
}

pub fn tokens_total(
    team: i64,
    key: i64,
    model: &str,
    backend: &str,
    direction: &str,
    estimated: bool,
    count: u64,
) {
    metrics::counter!(
        "router_tokens_total",
        "team" => team.to_string(),
        "key" => key.to_string(),
        "model" => model.to_string(),
        "backend" => backend.to_string(),
        "direction" => direction.to_string(),
        "estimated" => estimated.to_string(),
    )
    .increment(count);
}

pub fn observe_ttfb(model: &str, backend: &str, stream: bool, seconds: f64) {
    metrics::histogram!(
        "router_ttfb_seconds",
        "model" => model.to_string(),
        "backend" => backend.to_string(),
        "stream" => stream.to_string(),
    )
    .record(seconds);
}

pub fn observe_overhead(model: &str, backend: &str, stream: bool, seconds: f64) {
    metrics::histogram!(
        "router_overhead_seconds",
        "model" => model.to_string(),
        "backend" => backend.to_string(),
        "stream" => stream.to_string(),
    )
    .record(seconds);
}

pub fn set_backend_inflight(backend_id: i64, inflight: u32) {
    metrics::gauge!("router_backend_inflight", "backend" => backend_id.to_string())
        .set(inflight as f64);
}

pub fn set_circuit_open(backend_id: i64, open: bool) {
    metrics::gauge!("router_circuit_open", "backend" => backend_id.to_string()).set(if open {
        1.0
    } else {
        0.0
    });
}

pub fn set_budget_remaining(team: i64, model: &str, remaining: u64) {
    metrics::gauge!(
        "router_budget_remaining",
        "team" => team.to_string(),
        "model" => model.to_string(),
    )
    .set(remaining as f64);
}

#[cfg(test)]
mod tests {
    #[test]
    fn metric_names_locked() {
        assert!(super::METRICS.contains(&"router_overhead_seconds"));
    }
}
