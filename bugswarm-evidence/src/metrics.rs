use std::sync::atomic::{AtomicU64, Ordering};
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
