use std::sync::atomic::{AtomicU64, Ordering};
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
