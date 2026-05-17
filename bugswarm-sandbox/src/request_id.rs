use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_request_id() -> String {
    format!("req-{:016x}", NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed))
}
