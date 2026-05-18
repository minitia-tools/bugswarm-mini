use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct EvidenceMetrics {
    pub connections_total: AtomicU64,
    pub requests_total: AtomicU64,
    pub claims_total: AtomicU64,
    pub sandbox_runs_total: AtomicU64,
    pub bugs_confirmed_total: AtomicU64,
    pub integrity_checks_total: AtomicU64,
    pub start_time: Instant,
}

impl EvidenceMetrics {
    pub fn new() -> Self {
        Self {
            connections_total: AtomicU64::new(0),
            requests_total: AtomicU64::new(0),
            claims_total: AtomicU64::new(0),
            sandbox_runs_total: AtomicU64::new(0),
            bugs_confirmed_total: AtomicU64::new(0),
            integrity_checks_total: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn render_prometheus(&self) -> String {
        let uptime = self.start_time.elapsed().as_secs();
        format!(
            "# HELP bugswarm_uptime_seconds Evidence daemon uptime\n\
             # TYPE bugswarm_uptime_seconds gauge\n\
             bugswarm_uptime_seconds {}\n\
             # HELP bugswarm_evidence_requests_total Total requests\n\
             # TYPE bugswarm_evidence_requests_total counter\n\
             bugswarm_evidence_requests_total {}\n\
             # HELP bugswarm_evidence_connections_total Total connections\n\
             # TYPE bugswarm_evidence_connections_total counter\n\
             bugswarm_evidence_connections_total {}\n\
             # HELP bugswarm_evidence_claims_total Total claims added\n\
             # TYPE bugswarm_evidence_claims_total counter\n\
             bugswarm_evidence_claims_total {}\n\
             # HELP bugswarm_evidence_sandbox_runs_total Total sandbox runs\n\
             # TYPE bugswarm_evidence_sandbox_runs_total counter\n\
             bugswarm_evidence_sandbox_runs_total {}\n\
             # HELP bugswarm_evidence_bugs_confirmed_total Total bugs confirmed\n\
             # TYPE bugswarm_evidence_bugs_confirmed_total counter\n\
             bugswarm_evidence_bugs_confirmed_total {}\n\
             # HELP bugswarm_evidence_integrity_checks_total Total integrity checks\n\
             # TYPE bugswarm_evidence_integrity_checks_total counter\n\
             bugswarm_evidence_integrity_checks_total {}\n",
            uptime,
            self.requests_total.load(Ordering::Relaxed),
            self.connections_total.load(Ordering::Relaxed),
            self.claims_total.load(Ordering::Relaxed),
            self.sandbox_runs_total.load(Ordering::Relaxed),
            self.bugs_confirmed_total.load(Ordering::Relaxed),
            self.integrity_checks_total.load(Ordering::Relaxed),
        )
    }
}

pub async fn spawn_http_server(port: u16, metrics: Arc<EvidenceMetrics>) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Evidence HTTP server bind failed on {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("Evidence HTTP health/metrics on {}", addr);
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
                        let chroma_path = "/var/lib/bugswarm/chroma";
                        let chroma_ok = std::path::Path::new(chroma_path).exists();
                        let ready_body = if chroma_ok {
                            r#"{"status":"ready","chroma":true}"#.to_string()
                        } else {
                            r#"{"status":"not_ready","chroma":false}"#.to_string()
                        };
                        (if chroma_ok { "200 OK" } else { "503 Service Unavailable" },
                         "application/json", ready_body)
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
                tracing::warn!("Evidence HTTP accept error: {}", e);
            }
        }
    }
}
