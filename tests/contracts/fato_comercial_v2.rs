use ap_contracts::{
    load_fato_comercial_v2, load_schema, validate_fato_comercial_v2, validate_instance,
    verify_release,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn examples(folder: &str) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v2/examples")
        .join(folder);
    let mut paths: Vec<_> = fs::read_dir(root)
        .expect("examples")
        .map(|entry| entry.expect("entry").path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| serde_json::from_str(&fs::read_to_string(path).expect("read")).expect("json"))
        .collect()
}

#[test]
fn v1_remains_accepted_and_published() {
    verify_release().expect("v1 release");
    let schema = load_schema("fato-comercial").expect("v1 schema");
    let fixture: Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/v1/fixtures/valid/fato-comercial.json"),
        )
        .expect("v1 fixture"),
    )
    .expect("json");
    validate_instance(&schema, &fixture).expect("v1 accepted");
}

#[test]
fn v2_schema_matches_published_hash() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/v2");
    let schema = fs::read(root.join("fato-comercial.schema.json")).expect("schema");
    let actual = format!(
        "{:x}  fato-comercial.schema.json\n",
        Sha256::digest(&schema)
    );
    assert_eq!(
        fs::read_to_string(root.join("manifest.sha256")).expect("manifest"),
        actual
    );
}

#[test]
fn service_product_mixed_and_bundled_examples_close() {
    let schema = load_fato_comercial_v2().expect("v2 schema");
    let fixtures = examples("valid");
    assert_eq!(fixtures.len(), 4);
    for fact in fixtures {
        validate_fato_comercial_v2(&schema, &fact).expect("valid fact");
    }
}

#[test]
fn malformed_and_inconsistent_examples_fail() {
    let schema = load_fato_comercial_v2().expect("v2 schema");
    let fixtures = examples("invalid");
    assert_eq!(fixtures.len(), 5);
    for fact in fixtures {
        assert!(validate_fato_comercial_v2(&schema, &fact).is_err());
    }
}

#[test]
fn duplicate_items_and_overflow_fail() {
    let schema = load_fato_comercial_v2().expect("v2 schema");
    let mut fact = examples("valid").remove(0);
    let duplicate = fact["items"][0].clone();
    fact["items"].as_array_mut().expect("items").push(duplicate);
    assert!(validate_fato_comercial_v2(&schema, &fact).is_err());
    let mut fact = examples("valid")
        .into_iter()
        .find(|fact| fact["pricing_mode"] == "itemized")
        .expect("itemized example");
    fact["items"][0]["quantity_milli"] = json!(i64::MAX);
    assert!(validate_fato_comercial_v2(&schema, &fact).is_err());
}

#[test]
fn bundled_total_must_close_without_item_prices() {
    let schema = load_fato_comercial_v2().expect("v2 schema");
    let mut fact = examples("valid")
        .into_iter()
        .find(|fact| fact["pricing_mode"] == "bundle")
        .expect("bundle example");
    fact["totals"]["total_cents"] = json!(319999);
    assert!(validate_fato_comercial_v2(&schema, &fact).is_err());
}
