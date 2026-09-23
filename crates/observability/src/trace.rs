use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use tracing_subscriber::fmt::MakeWriter;

use crate::{
    evaluate, redact, Alert, CorrelationId, EventId, MetricSnapshot, Metrics, TenantPseudonym,
};

pub struct SyntheticInput<'a> {
    pub raw_tenant: &'a str,
    pub unsafe_detail: &'a str,
    pub queue_depth: u64,
    pub retries: u64,
    pub indeterminate: u64,
    pub provider_failures: u64,
}

pub struct SyntheticObservation {
    pub correlation_id: CorrelationId,
    pub event_id: EventId,
    pub tenant: TenantPseudonym,
    pub redacted_detail: String,
    pub alerts: Vec<Alert>,
    pub snapshot: MetricSnapshot,
}

#[must_use]
pub fn observe_synthetic(input: SyntheticInput<'_>) -> SyntheticObservation {
    let correlation_id = CorrelationId::generate();
    let event_id = EventId::generate();
    let tenant = TenantPseudonym::from_raw(input.raw_tenant);
    let redacted_detail = redact(input.unsafe_detail);
    let mut metrics = Metrics::default();
    metrics.set_queue_depth(input.queue_depth);
    metrics.record_retries(input.retries);
    for _ in 0..input.indeterminate {
        metrics.record_indeterminate();
    }
    for _ in 0..input.provider_failures {
        metrics.record_provider_failure();
    }
    let snapshot = metrics.snapshot();
    let alerts = evaluate(&snapshot, &correlation_id);
    tracing::info!(
        correlation_id = %correlation_id,
        event_id = %event_id,
        tenant = %tenant,
        detail = %redacted_detail,
        queue_depth = snapshot.queue_depth,
        retries = snapshot.retries,
        indeterminate = snapshot.indeterminate,
        provider_failures = snapshot.provider_failures,
        alert_count = alerts.len(),
        "observacao sintetica"
    );
    SyntheticObservation {
        correlation_id,
        event_id,
        tenant,
        redacted_detail,
        alerts,
        snapshot,
    }
}

/// Coletor em memória. Substitui um backend externo neste ticket.
#[must_use]
pub fn capture_trace(action: impl FnOnce()) -> String {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .without_time()
        .with_writer(SharedBuf(Arc::clone(&buffer)))
        .finish();
    tracing::subscriber::with_default(subscriber, action);
    let bytes = buffer
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for SharedBuf {
    type Writer = SharedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriter(Arc::clone(&self.0))
    }
}
