//! Fronteira do módulo `audit`.
//!
//! O evento não tem campo de payload. Documento, token e XML ficam de fora.
//! Este crate não referencia outros domínios.

use serde::Serialize;

use ap_observability::{is_opaque_token, CorrelationId, EventId, TenantPseudonym};

ap_kernel::declare_module!("audit");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    Indeterminate,
}

impl Outcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Indeterminate => "indeterminate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    UnsafeIdentifier,
}

/// Registro seguro. Os campos são fechados de propósito.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEvent {
    correlation_id: String,
    event_id: String,
    tenant_pseudonym: String,
    action: &'static str,
    outcome: &'static str,
}

impl AuditEvent {
    pub fn process_observation(
        correlation_id: &CorrelationId,
        event_id: &EventId,
        tenant: &TenantPseudonym,
        outcome: Outcome,
    ) -> Result<Self, AuditError> {
        Self::try_process_observation(
            correlation_id.as_str(),
            event_id.as_str(),
            tenant.as_str(),
            outcome,
        )
    }

    pub fn try_process_observation(
        correlation_id: &str,
        event_id: &str,
        tenant_pseudonym: &str,
        outcome: Outcome,
    ) -> Result<Self, AuditError> {
        if !is_opaque_token(correlation_id)
            || !is_opaque_token(event_id)
            || !is_pseudonym(tenant_pseudonym)
        {
            return Err(AuditError::UnsafeIdentifier);
        }
        Ok(Self {
            correlation_id: correlation_id.to_string(),
            event_id: event_id.to_string(),
            tenant_pseudonym: tenant_pseudonym.to_string(),
            action: "process_observation",
            outcome: outcome.as_str(),
        })
    }
}

fn is_pseudonym(value: &str) -> bool {
    value == "absent" || (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use ap_observability::{
        capture_trace, observe_synthetic, CorrelationId, EventId, SyntheticInput, TenantPseudonym,
        QUEUE_BACKLOG_THRESHOLD, RETRY_EXHAUSTED_THRESHOLD,
    };

    use super::{AuditError, AuditEvent, Outcome};

    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }

    #[test]
    fn rejects_raw_tenant_document_and_secret() {
        let secret = ["super", "-secret-token"].concat();
        let cases = [
            ("curto", "trace-synthetic-0001", "absent"),
            (
                "trace-synthetic-0001",
                "trace-synthetic-0001",
                "tenant-raw-synthetic",
            ),
            (secret.as_str(), "trace-synthetic-0001", "absent"),
        ];
        for (correlation_id, event_id, tenant) in cases {
            let error = AuditEvent::try_process_observation(
                correlation_id,
                event_id,
                tenant,
                Outcome::Failure,
            );
            assert_eq!(error, Err(AuditError::UnsafeIdentifier));
        }
    }

    #[test]
    fn serialized_event_has_no_payload_and_trace_stays_clean() {
        let raw_tenant = "tenant-raw-synthetic";
        let secret = ["doc", "-secret-marker"].concat();
        let xml = ["<NFe>", secret.as_str(), "</NFe>"].concat();
        let slot = std::sync::Mutex::new(None);
        let trace = capture_trace(|| {
            let observation = observe_synthetic(SyntheticInput {
                raw_tenant,
                unsafe_detail: &xml,
                queue_depth: QUEUE_BACKLOG_THRESHOLD,
                retries: RETRY_EXHAUSTED_THRESHOLD,
                indeterminate: 1,
                provider_failures: 1,
            });
            let event = AuditEvent::process_observation(
                &observation.correlation_id,
                &observation.event_id,
                &observation.tenant,
                Outcome::Indeterminate,
            )
            .expect("evento");
            let encoded = serde_json::to_string(&event).expect("json");
            assert!(encoded.contains("process_observation"));
            assert!(encoded.contains("indeterminate"));
            assert!(!encoded.contains("payload"));
            assert!(!encoded.contains("document"));
            assert!(!encoded.contains(&secret));
            assert!(!encoded.contains(raw_tenant));
            tracing::info!(audit = %encoded, "evento de auditoria");
            *slot.lock().expect("slot") = Some((
                observation.correlation_id.clone(),
                observation.event_id.clone(),
                observation.tenant.clone(),
            ));
        });
        let (correlation_id, event_id, tenant) = slot.lock().expect("slot").take().expect("ids");
        assert!(trace.contains(correlation_id.as_str()));
        assert!(trace.contains(event_id.as_str()));
        assert!(trace.contains(tenant.as_str()));
        assert!(!trace.contains(raw_tenant));
        assert!(!trace.contains(&secret));
        let absent = TenantPseudonym::from_raw("");
        let event = AuditEvent::process_observation(
            &CorrelationId::generate(),
            &EventId::generate(),
            &absent,
            Outcome::Success,
        )
        .expect("ausente");
        assert_eq!(event.tenant_pseudonym, "absent");
    }
}
