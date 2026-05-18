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

pub async fn spawn_http_server(port: u16, metrics: Arc<CpgMetrics>) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("CPG HTTP server bind failed on {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("CPG HTTP health/metrics on {}", addr);
    loop {
        match listener.accept().await {
            Ok((mut socket, _)) => {
                let m = metrics.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let n = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let (status, content_type, body) = if request.contains("GET /metrics") {
                        ("200 OK", "text/plain; charset=utf-8", m.render_prometheus())
                    } else if request.contains("GET /ready") {
                        ("200 OK", "application/json", r#"{"status":"ready"}"#.to_string())
                    } else {
                        ("200 OK", "application/json", r#"{"status":"healthy"}"#.to_string())
                    };
                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status, content_type, body.len(), body
                    );
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
                });
            }
            Err(e) => {
                tracing::warn!("CPG HTTP accept error: {}", e);
            }
        }
    }
}
