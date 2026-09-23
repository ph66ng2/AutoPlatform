use std::{fs, path::Path};

use ap_contracts::{load_schema, validate_instance};
use ap_gateway_spike::{
    after_attempt, evaluate, live_sandbox, load_matrix, on_checkout_return, retry, verify_asaas,
    verify_mercado_pago, Attempt, ConfirmError, Context, IdempotencyStore, LiveError, Outcome,
    Provider, ProviderState, Secret, WorkshopOAuth, ASAAS_TOKEN_HEADER, MP_SIGNATURE_HEADER,
    PASS_SCORE,
};
use serde_json::json;

#[test]
fn signed_webhooks_reject_tampering() {
    let asaas_token = "synthetic-asaas-webhook";
    verify_asaas(Some(asaas_token), asaas_token).expect("asaas");
    assert!(verify_asaas(None, asaas_token).is_err());
    assert!(verify_asaas(Some("outro"), asaas_token).is_err());
    assert_eq!(ASAAS_TOKEN_HEADER, "asaas-access-token");

    let secret = "synthetic-mp-webhook";
    let payload = "{\"id\":\"evt-1\"}";
    let ts = "1700000000";
    let mac =
        ap_gateway_spike::hmac_sha256(secret.as_bytes(), format!("{ts}.{payload}").as_bytes());
    let header = format!("ts={ts},v1={}", hex_of(&mac));
    verify_mercado_pago(Some(&header), payload, secret).expect("mp");
    assert!(verify_mercado_pago(None, payload, secret).is_err());
    assert!(verify_mercado_pago(Some("ts=1,v1=00"), payload, secret).is_err());
    assert_eq!(MP_SIGNATURE_HEADER, "x-signature");
}

#[test]
fn rfc4231_hmac_sha256_case1() {
    let key = [0x0b; 20];
    let mac = ap_gateway_spike::hmac_sha256(&key, b"Hi There");
    assert_eq!(
        hex_of(&mac),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn idempotency_timeout_redirect_and_oauth_stay_on_the_server() {
    let mut store = IdempotencyStore::default();
    let billing = store.apply("idem-platform-bill01", 9900);
    let first = store.apply("idem-workshop-pay01", 15000);
    let second = store.apply("idem-workshop-pay01", 15000);
    assert_eq!(billing, 9900);
    assert_eq!(first, 15000);
    assert_eq!(second, 15000);
    assert_eq!(store.total(), 24900);

    assert_eq!(
        after_attempt(Attempt::Timeout),
        ProviderState::Indeterminate
    );
    assert!(retry(ProviderState::Indeterminate).is_err());
    assert!(retry(ProviderState::Failed).is_ok());

    assert_eq!(
        on_checkout_return("status=approved"),
        Err(ConfirmError::RedirectDoesNotConfirm)
    );

    let oauth = WorkshopOAuth::exchange("access-synthetic-token", "refresh-synthetic-token");
    let desktop = oauth.desktop_view();
    assert!(desktop.authorized);
    let dumped = format!("{oauth:?}{desktop:?}");
    assert!(!dumped.contains("access-synthetic-token"));
    assert!(!dumped.contains("refresh-synthetic-token"));
    assert_eq!(format!("{}", Secret::new("x".into())), "[REDACTED]");
    assert_ne!(Outcome::Refunded, Outcome::ChargedBack);
}

#[test]
fn payment_intent_contract_and_live_sandbox_stay_closed() {
    let schema = load_schema("payment-intent").expect("schema");
    let mut intent = json!({
        "schema_version": "1",
        "company_id": "00000000-0000-4000-8000-0000000000a1",
        "correlation_id": "corr-payment-intent01",
        "event_id": "event-payment-intent",
        "idempotency_key": "idem-payment-intent01",
        "payment_intent_id": "00000000-0000-4000-8000-0000000000c1",
        "amount_cents": 15000,
        "method": "pix",
        "status": "pending",
        "occurred_at": "2026-09-23T12:00:03Z"
    });
    assert!(validate_instance(&schema, &intent).is_ok());
    intent["method"] = json!("card");
    assert!(validate_instance(&schema, &intent).is_ok());
    assert_eq!(live_sandbox(), Err(LiveError::NotAuthorized));
}

#[test]
fn matrix_keeps_contexts_apart_and_selects_nobody() {
    let matrix = load_matrix();
    assert_eq!(matrix.pass_score, PASS_SCORE);
    let weight_sum: u8 = matrix.weights.values().sum();
    assert_eq!(weight_sum, 100);
    assert!(matrix.blockers.contains(&"contexts_separated".into()));
    let cards = evaluate(&matrix, true);
    for context in [Context::PlatformBilling, Context::WorkshopPayments] {
        for provider in [Provider::Asaas, Provider::MercadoPago] {
            let card = cards.get(&(context, provider)).expect("card");
            assert!(card.synthetic_blockers);
            assert!(!card.live_executed);
            assert_eq!(card.live_score, None);
            assert_eq!(card.selected, None);
        }
    }
    let adr =
        fs::read_to_string(repo_root().join("docs/adr/0001-billing-e-pagamentos.md")).expect("adr");
    assert!(adr.contains("Nenhum provider foi escolhido"));
    assert!(adr.contains("platform_billing"));
    assert!(adr.contains("workshop_payments"));
}

#[test]
fn desktop_clients_do_not_embed_provider_tokens() {
    let root = repo_root();
    let clients = [
        root.join("examples"),
        root.join("openapi"),
        root.join("crates/portal"),
    ];
    let forbidden = [
        "APP_USR-",
        "access_token=",
        "refresh_token=",
        "asaas-access-token",
    ];
    for dir in clients {
        for path in walk_files(&dir) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            for needle in forbidden {
                assert!(
                    !text.contains(needle),
                    "{} contém material de provider",
                    path.strip_prefix(&root).unwrap_or(&path).display()
                );
            }
        }
    }
}

fn hex_of(bytes: &[u8]) -> String {
    ap_gateway_spike::hex_lower(bytes)
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}
