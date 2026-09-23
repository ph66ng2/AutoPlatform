use crate::{CorrelationId, MetricSnapshot};

pub const QUEUE_BACKLOG_THRESHOLD: u64 = 1;
pub const RETRY_EXHAUSTED_THRESHOLD: u64 = 3;

pub struct AlertSpec {
    pub id: &'static str,
    pub owner: &'static str,
    pub runbook: &'static str,
    fires: fn(&MetricSnapshot) -> bool,
}

pub struct Alert {
    pub id: &'static str,
    pub owner: &'static str,
    pub runbook: &'static str,
    pub correlation_id: String,
}

pub const BASE_ALERTS: &[AlertSpec] = &[
    AlertSpec {
        id: "provider_failure",
        owner: "plataforma",
        runbook: "docs/runbooks/falha-de-provider.md",
        fires: |snapshot| snapshot.provider_failures > 0,
    },
    AlertSpec {
        id: "indeterminate_outcome",
        owner: "plataforma",
        runbook: "docs/runbooks/resultado-indeterminado.md",
        fires: |snapshot| snapshot.indeterminate > 0,
    },
    AlertSpec {
        id: "queue_backlog",
        owner: "plataforma",
        runbook: "docs/runbooks/fila-acumulada.md",
        fires: |snapshot| snapshot.queue_depth >= QUEUE_BACKLOG_THRESHOLD,
    },
    AlertSpec {
        id: "retry_exhausted",
        owner: "plataforma",
        runbook: "docs/runbooks/retries-esgotados.md",
        fires: |snapshot| snapshot.retries >= RETRY_EXHAUSTED_THRESHOLD,
    },
];

#[must_use]
pub fn evaluate(snapshot: &MetricSnapshot, correlation_id: &CorrelationId) -> Vec<Alert> {
    BASE_ALERTS
        .iter()
        .filter(|spec| (spec.fires)(snapshot))
        .map(|spec| Alert {
            id: spec.id,
            owner: spec.owner,
            runbook: spec.runbook,
            correlation_id: correlation_id.as_str().to_string(),
        })
        .collect()
}
