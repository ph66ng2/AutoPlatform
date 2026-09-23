use serde::Serialize;

/// Contadores operacionais. Não guardam payload de provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MetricSnapshot {
    pub queue_depth: u64,
    pub retries: u64,
    pub indeterminate: u64,
    pub provider_failures: u64,
}

#[derive(Debug, Default)]
pub struct Metrics {
    snapshot: MetricSnapshot,
}

impl Metrics {
    #[must_use]
    pub fn snapshot(&self) -> MetricSnapshot {
        self.snapshot
    }

    pub fn set_queue_depth(&mut self, depth: u64) {
        self.snapshot.queue_depth = depth;
    }

    pub fn record_retries(&mut self, count: u64) {
        self.snapshot.retries = self.snapshot.retries.saturating_add(count);
    }

    pub fn record_indeterminate(&mut self) {
        self.snapshot.indeterminate = self.snapshot.indeterminate.saturating_add(1);
    }

    pub fn record_provider_failure(&mut self) {
        self.snapshot.provider_failures = self.snapshot.provider_failures.saturating_add(1);
    }
}
