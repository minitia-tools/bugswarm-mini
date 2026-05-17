use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct DaemonMetrics {
    pub connections_total: AtomicU64,
    pub requests_total: AtomicU64,
    pub executions_total: AtomicU64,
    pub execution_errors_total: AtomicU64,
    pub fuzz_campaigns_total: AtomicU64,
    pub delta_runs_total: AtomicU64,
    pub mutation_runs_total: AtomicU64,
    pub invariant_runs_total: AtomicU64,
    pub start_time: Instant,
}

impl DaemonMetrics {
    pub fn new() -> Self {
        Self {
            connections_total: AtomicU64::new(0),
            requests_total: AtomicU64::new(0),
            executions_total: AtomicU64::new(0),
            execution_errors_total: AtomicU64::new(0),
            fuzz_campaigns_total: AtomicU64::new(0),
            delta_runs_total: AtomicU64::new(0),
            mutation_runs_total: AtomicU64::new(0),
            invariant_runs_total: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn render_prometheus(&self) -> String {
        let uptime = self.start_time.elapsed().as_secs();
        format!(
            "# HELP bugswarm_uptime_seconds Daemon uptime\n\
             # TYPE bugswarm_uptime_seconds gauge\n\
             bugswarm_uptime_seconds {}\n\
             # HELP bugswarm_requests_total Total requests\n\
             # TYPE bugswarm_requests_total counter\n\
             bugswarm_requests_total {}\n\
             # HELP bugswarm_executions_total Total sandbox executions\n\
             # TYPE bugswarm_executions_total counter\n\
             bugswarm_executions_total {}\n\
             # HELP bugswarm_execution_errors_total Total execution errors\n\
             # TYPE bugswarm_execution_errors_total counter\n\
             bugswarm_execution_errors_total {}\n\
             # HELP bugswarm_connections_total Total connections\n\
             # TYPE bugswarm_connections_total counter\n\
             bugswarm_connections_total {}\n\
             # HELP bugswarm_fuzz_campaigns_total Total fuzz campaigns\n\
             # TYPE bugswarm_fuzz_campaigns_total counter\n\
             bugswarm_fuzz_campaigns_total {}\n\
             # HELP bugswarm_delta_runs_total Total delta debugging runs\n\
             # TYPE bugswarm_delta_runs_total counter\n\
             bugswarm_delta_runs_total {}\n\
             # HELP bugswarm_mutation_runs_total Total mutation testing runs\n\
             # TYPE bugswarm_mutation_runs_total counter\n\
             bugswarm_mutation_runs_total {}\n\
             # HELP bugswarm_invariant_runs_total Total invariant mining runs\n\
             # TYPE bugswarm_invariant_runs_total counter\n\
             bugswarm_invariant_runs_total {}\n",
            uptime,
            self.requests_total.load(Ordering::Relaxed),
            self.executions_total.load(Ordering::Relaxed),
            self.execution_errors_total.load(Ordering::Relaxed),
            self.connections_total.load(Ordering::Relaxed),
            self.fuzz_campaigns_total.load(Ordering::Relaxed),
            self.delta_runs_total.load(Ordering::Relaxed),
            self.mutation_runs_total.load(Ordering::Relaxed),
            self.invariant_runs_total.load(Ordering::Relaxed),
        )
    }
}
