use ap_contracts::{
    assert_compatible, extract_surface, load_schema, published_from_disk, validate_instance,
    verify_release,
};
use serde_json::{json, Value};

#[test]
fn release_is_self_consistent() {
    verify_release().expect("release");
}

#[test]
fn envelope_and_unknown_version_are_rejected() {
    let schema = load_schema("fato-comercial").expect("schema");
    let mut valid = valid_fixture("fato-comercial");
    assert!(validate_instance(&schema, &valid).is_ok());
    valid["schema_version"] = json!("9");
    let error = validate_instance(&schema, &valid).expect_err("versão");
    assert!(format!("{error}").contains("schema_version"));
    valid.as_object_mut().expect("obj").remove("company_id");
    valid["schema_version"] = json!("1");
    assert!(validate_instance(&schema, &valid).is_err());
}

#[test]
fn breaking_change_is_detected_without_partial_accept() {
    let current = published_from_disk().expect("atual");
    let mut broken = current.clone();
    broken
        .0
        .get_mut("fato-comercial")
        .expect("fato")
        .required
        .push("novo_obrigatorio".into());
    assert!(assert_compatible(
        current.0.get("fato-comercial").expect("pub"),
        broken.0.get("fato-comercial").expect("brk"),
    )
    .is_err());

    let mut schema = load_schema("payment-intent").expect("schema");
    schema["properties"]["method"]["enum"] = json!(["pix"]);
    let extracted = extract_surface(&schema).expect("surface");
    let published = published_from_disk().expect("atual");
    assert!(
        assert_compatible(published.0.get("payment-intent").expect("pub"), &extracted).is_err()
    );
}

#[test]
fn consumers_share_the_published_hashes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let published =
        std::fs::read_to_string(root.join("contracts/v1/manifest.sha256")).expect("manifest");
    for consumer in ["autoos-consumer", "autobo-consumer"] {
        let expected =
            std::fs::read_to_string(root.join("examples").join(consumer).join("expected.sha256"))
                .unwrap_or_else(|error| panic!("{consumer}: {error}"));
        assert_eq!(expected, published, "{consumer}");
    }
}

fn valid_fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v1/fixtures/valid")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}
