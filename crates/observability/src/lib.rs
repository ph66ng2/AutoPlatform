//! Telemetria do processo: correlação, redação, métricas e alertas.
//!
//! Documento, token, XML e segredo não entram em log, trace ou alerta.
//! Não há coletor externo nesta entrega.

mod alert;
mod context;
mod metrics;
mod redact;
mod trace;

pub use alert::{
    evaluate, Alert, AlertSpec, BASE_ALERTS, QUEUE_BACKLOG_THRESHOLD, RETRY_EXHAUSTED_THRESHOLD,
};
pub use context::{
    is_opaque_token, CorrelationId, EventId, TenantPseudonym, HEADER_CORRELATION_ID,
    HEADER_EVENT_ID, HEADER_TENANT_KEY, HEADER_TENANT_PSEUDONYM,
};
pub use metrics::{MetricSnapshot, Metrics};
pub use redact::{contains_sensitive, redact};
pub use trace::{capture_trace, observe_synthetic, SyntheticInput, SyntheticObservation};

ap_kernel::declare_module!("observability");

#[cfg(test)]
mod tests {
    use super::{
        capture_trace, contains_sensitive, evaluate, observe_synthetic, redact, CorrelationId,
        MetricSnapshot, Metrics, SyntheticInput, TenantPseudonym, BASE_ALERTS,
        QUEUE_BACKLOG_THRESHOLD, RETRY_EXHAUSTED_THRESHOLD,
    };

    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
    }

    #[test]
    fn redacts_contact_document_credential_and_xml() {
        let email = "pessoa@example.com";
        let cpf = "111.444.777-35";
        let cnpj = "00.000.000/0000-00";
        let digits = "11144477735";
        let secret = ["super", "-secret-token"].concat();
        let url = ["postgres://", "user:", secret.as_str(), "@db/app"].concat();
        let bearer = ["Bearer ", secret.as_str()].concat();
        let assignment = ["password=", secret.as_str()].concat();
        let xml = ["<NFe>", secret.as_str(), "</NFe>"].concat();
        let private_key = ["BEGIN ", "PRIVATE KEY"].concat();

        for sample in [
            email,
            cpf,
            cnpj,
            digits,
            &url,
            &bearer,
            &assignment,
            &xml,
            &private_key,
        ] {
            let redacted = redact(sample);
            assert!(contains_sensitive(sample), "{sample}");
            assert!(!redacted.contains(secret.as_str()), "{redacted}");
            assert!(!redacted.contains("pessoa@example.com"), "{redacted}");
            assert!(!redacted.contains(cpf), "{redacted}");
            assert!(!redacted.contains(cnpj), "{redacted}");
            assert!(!redacted.contains(digits), "{redacted}");
            assert!(redacted.contains("[REDACTED]"), "{redacted}");
        }
        assert_eq!(redact("fila vazia no worker"), "fila vazia no worker");
    }

    #[test]
    fn header_correlation_rejects_unsafe_values() {
        let secret = ["super", "-secret-token"].concat();
        let incoming = CorrelationId::from_header(Some("trace-synthetic-0001"));
        assert_eq!(incoming.as_str(), "trace-synthetic-0001");
        let replaced = CorrelationId::from_header(Some(&secret));
        assert!(!replaced.as_str().contains(&secret));
        assert!(replaced.as_str().len() >= 16);
    }

    #[test]
    fn tenant_pseudonym_hides_the_raw_key() {
        let raw = "tenant-raw-synthetic";
        let pseudonym = TenantPseudonym::from_raw(raw);
        assert_eq!(pseudonym, TenantPseudonym::from_raw(raw));
        assert_ne!(pseudonym.as_str(), raw);
        assert!(!pseudonym.as_str().contains(raw));
        assert_eq!(TenantPseudonym::from_raw("").as_str(), "absent");
    }

    #[test]
    fn alerts_fire_only_at_the_base_thresholds() {
        let quiet = evaluate(&MetricSnapshot::default(), &CorrelationId::generate());
        assert!(quiet.is_empty());

        let mut metrics = Metrics::default();
        metrics.set_queue_depth(QUEUE_BACKLOG_THRESHOLD);
        metrics.record_retries(RETRY_EXHAUSTED_THRESHOLD);
        metrics.record_indeterminate();
        metrics.record_provider_failure();
        let correlation = CorrelationId::generate();
        let fired = evaluate(&metrics.snapshot(), &correlation);
        let ids: Vec<&str> = fired.iter().map(|alert| alert.id).collect();
        assert_eq!(
            ids,
            [
                "provider_failure",
                "indeterminate_outcome",
                "queue_backlog",
                "retry_exhausted",
            ]
        );
        assert!(fired
            .iter()
            .all(|alert| alert.correlation_id == correlation.as_str()));
    }

    #[test]
    fn base_alerts_have_owner_and_runbook() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(!BASE_ALERTS.is_empty());
        for spec in BASE_ALERTS {
            assert_eq!(spec.owner, "plataforma");
            let text = std::fs::read_to_string(root.join(spec.runbook))
                .unwrap_or_else(|err| panic!("{}: {err}", spec.runbook));
            assert!(text.contains("Owner: plataforma"), "{}", spec.runbook);
            assert!(text.contains(spec.id), "{}", spec.runbook);
            assert!(text.contains("## Ação"), "{}", spec.runbook);
        }
    }

    #[test]
    fn synthetic_trace_keeps_correlation_and_drops_the_payload() {
        let raw = "tenant-raw-synthetic";
        let secret = ["doc", "-secret-marker"].concat();
        let xml = ["<NFe>", secret.as_str(), "</NFe>"].concat();
        let slot = std::sync::Mutex::new(None);
        let trace = capture_trace(|| {
            let observation = observe_synthetic(SyntheticInput {
                raw_tenant: raw,
                unsafe_detail: &xml,
                queue_depth: QUEUE_BACKLOG_THRESHOLD,
                retries: RETRY_EXHAUSTED_THRESHOLD,
                indeterminate: 1,
                provider_failures: 1,
            });
            *slot.lock().expect("slot") = Some(observation);
        });
        let observation = slot.lock().expect("slot").take().expect("observacao");
        assert!(trace.contains(observation.correlation_id.as_str()));
        assert!(trace.contains(observation.event_id.as_str()));
        assert!(trace.contains(observation.tenant.as_str()));
        assert!(!trace.contains(raw));
        assert!(!trace.contains(&secret));
        assert!(!observation.redacted_detail.contains(&secret));
        assert_eq!(observation.alerts.len(), 4);
    }
}
