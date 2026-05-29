use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct CpgMetrics {
    pub connections_total: AtomicU64,
    pub requests_total: AtomicU64,
    pub index_requests_total: AtomicU64,
    pub taint_queries_total: AtomicU64,
    pub danger_map_requests_total: AtomicU64,
    pub start_time: Instant,
}

impl CpgMetrics {
    pub fn new() -> Self {
        Self {
            connections_total: AtomicU64::new(0),
            requests_total: AtomicU64::new(0),
            index_requests_total: AtomicU64::new(0),
            taint_queries_total: AtomicU64::new(0),
            danger_map_requests_total: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn render_prometheus(&self) -> String {
        let uptime = self.start_time.elapsed().as_secs();
        format!(
            "# HELP bugswarm_uptime_seconds CPG daemon uptime\n\
             # TYPE bugswarm_uptime_seconds gauge\n\
             bugswarm_uptime_seconds {}\n\
             # HELP bugswarm_cpg_requests_total Total requests\n\
             # TYPE bugswarm_cpg_requests_total counter\n\
             bugswarm_cpg_requests_total {}\n\
             # HELP bugswarm_cpg_connections_total Total connections\n\
             # TYPE bugswarm_cpg_connections_total counter\n\
             bugswarm_cpg_connections_total {}\n\
             # HELP bugswarm_cpg_index_requests_total Total index requests\n\
             # TYPE bugswarm_cpg_index_requests_total counter\n\
             bugswarm_cpg_index_requests_total {}\n\
             # HELP bugswarm_cpg_taint_queries_total Total taint queries\n\
             # TYPE bugswarm_cpg_taint_queries_total counter\n\
             bugswarm_cpg_taint_queries_total {}\n\
             # HELP bugswarm_cpg_danger_map_requests_total Total danger map requests\n\
             # TYPE bugswarm_cpg_danger_map_requests_total counter\n\
             bugswarm_cpg_danger_map_requests_total {}\n",
            uptime,
            self.requests_total.load(Ordering::Relaxed),
            self.connections_total.load(Ordering::Relaxed),
            self.index_requests_total.load(Ordering::Relaxed),
            self.taint_queries_total.load(Ordering::Relaxed),
            self.danger_map_requests_total.load(Ordering::Relaxed),
        )
    }
}

impl Default for CpgMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn spawn_http_server(port: u16, metrics: Arc<CpgMetrics>) {
    use axum::{Router, routing::get, Json, http::StatusCode, extract::State};

    #[derive(Clone)]
    struct AppState { metrics: Arc<CpgMetrics> }

    async fn health() -> (StatusCode, Json<serde_json::Value>) {
        (StatusCode::OK, Json(serde_json::json!({"status":"healthy","version":env!("CARGO_PKG_VERSION"),"service":"bugswarm-cpg"})))
    }

    async fn ready() -> (StatusCode, Json<serde_json::Value>) {
        (StatusCode::OK, Json(serde_json::json!({"status":"ready"})))
    }

    async fn metrics_handler(State(state): State<AppState>) -> String {
        state.metrics.render_prometheus()
    }

    let state = AppState { metrics };
    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind metrics server on {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("CPG HTTP health/metrics server on {}", addr);
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Metrics server error: {}", e);
    }
}
