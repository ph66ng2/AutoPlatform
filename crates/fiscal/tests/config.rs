use std::{
    env, fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use ap_fiscal::{
    approve_decision, approve_item, approve_scenario, complete_issuer_profile,
    create_issuer_profile, prepare_plan, read_provider_secret, register_item, register_scenario,
    rotate_provider_secret, save_decision_draft, save_provider_secret, DecisionApproval,
    DocumentKind, DocumentPlan, FiscalError, MasterKey, PgDecisionResolver,
};
use ap_identity::{Product, Role, Session};
use postgres::{Client, NoTls};
use serde_json::Value;
use uuid::Uuid;

fn id(raw: &str) -> Uuid {
    Uuid::parse_str(raw).expect("synthetic uuid")
}

fn db(name: &str) -> Client {
    let mut config = postgres::Config::new();
    config
        .host(&env::var("PGHOST").expect("PGHOST"))
        .port(env::var("PGPORT").expect("PGPORT").parse().expect("port"))
        .user(&env::var("PGUSER").expect("PGUSER"))
        .password(env::var("PGPASSWORD").expect("PGPASSWORD"))
        .dbname(name);
    config.connect(NoTls).expect("ephemeral postgres")
}

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v2/examples/valid")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("synthetic fact")
}

#[test]
fn draft_approval_tenant_and_envelope_are_enforced() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let mut admin = db("postgres");
    admin
        .batch_execute("DROP DATABASE IF EXISTS fiscal_config")
        .expect("drop old ephemeral database");
    admin
        .batch_execute("CREATE DATABASE fiscal_config")
        .expect("create ephemeral database");
    drop(admin);
    let mut client = db("fiscal_config");
    for n in 1..=8 {
        let filename = match n {
            1 => "0001_baseline.sql",
            2 => "0002_identity.sql",
            3 => "0003_inventory.sql",
            4 => "0004_fiscal.sql",
            5 => "0005_fiscal_operation_recovery.sql",
            6 => "0006_commercial_fact_ingest.sql",
            7 => "0007_commercial_fact_keys.sql",
            _ => "0008_fiscal_config.sql",
        };
        let sql = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .join(filename),
        )
        .expect("migration");
        client.batch_execute(&sql).expect("additive migration");
    }
    client.batch_execute("INSERT INTO identity.companies(id,label,status) VALUES
        ('00000000-0000-4000-8000-0000000000a1','tenant a','active'),
        ('00000000-0000-4000-8000-0000000000d1','tenant b','active');
        INSERT INTO identity.accounts(id,status) VALUES
        ('00000000-0000-4000-8000-0000000000a2','active'),
        ('00000000-0000-4000-8000-0000000000d2','active');
        INSERT INTO identity.memberships(company_id,id,account_id,product,role,status) VALUES
        ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a3','00000000-0000-4000-8000-0000000000a2','autobo','fiscal','active'),
        ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d3','00000000-0000-4000-8000-0000000000d2','autobo','fiscal','active');
        INSERT INTO fiscal.issuer_profiles(company_id,id,establishment_id,label) VALUES
        ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d4','00000000-0000-4000-8000-0000000000d5','synthetic b');")
        .expect("seed synthetic tenants");

    let company = id("00000000-0000-4000-8000-0000000000a1");
    let establishment = id("00000000-0000-4000-8000-0000000000b1");
    let session = Session {
        company_id: company,
        account_id: id("00000000-0000-4000-8000-0000000000a2"),
        product: Product::AutoBo,
        role: Role::Fiscal,
    };
    let fact = fixture("servico.json");
    let fact_id = id(fact["fact_id"].as_str().expect("fact id"));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;
    create_issuer_profile(
        &mut client,
        session,
        id("00000000-0000-4000-8000-0000000000c1"),
        establishment,
        "synthetic a",
    )
    .expect("create profile");
    complete_issuer_profile(&mut client, session, establishment, "0000000", "synthetic")
        .expect("profile");
    register_item(
        &mut client,
        session,
        establishment,
        "servico-1",
        "Synthetic service",
        "service",
        "none",
    )
    .expect("item");
    approve_item(&mut client, session, establishment, "servico-1").expect("approve item");
    register_scenario(
        &mut client,
        session,
        establishment,
        "service-only",
        "service",
        "none",
        "itemized",
    )
    .expect("scenario");
    let total = fact["totals"]["total_cents"].as_i64().expect("total");
    let documents = [DocumentPlan {
        key: "synthetic-service".into(),
        kind: DocumentKind::Nfse,
        amount_cents: total,
        tax_base_cents: total,
        fiscal_code: "synthetic-code".into(),
        item_ids: vec!["servico-1".into()],
    }];
    save_decision_draft(
        &mut client,
        session,
        establishment,
        "service-only",
        fact_id,
        1,
        &documents,
    )
    .expect("draft");
    let resolver = PgDecisionResolver::new(db("fiscal_config"), session);
    assert!(matches!(
        prepare_plan(session, &fact, "service-only", now + 10, &resolver),
        Err(FiscalError::DecisionMissing)
    ));
    approve_scenario(
        &mut client,
        session,
        establishment,
        "service-only",
        now - 10,
        Some(now + 20),
        "synthetic-review",
    )
    .expect("approve scenario");
    approve_decision(
        &mut client,
        session,
        DecisionApproval {
            establishment_id: establishment,
            scenario: "service-only",
            fact_id,
            version: 1,
            effective_from_epoch: now - 10,
            effective_until_epoch: Some(now + 20),
            evidence_ref: "synthetic-review",
        },
    )
    .expect("approve decision");
    prepare_plan(session, &fact, "service-only", now + 10, &resolver)
        .expect("approved synthetic decision");
    let mut customer_fact = fixture("servico.json");
    let customer_fact_id = Uuid::new_v4();
    customer_fact["fact_id"] = Value::String(customer_fact_id.to_string());
    customer_fact["items"][0]["item_id"] = Value::String("service-customer-piece".into());
    register_item(
        &mut client,
        session,
        establishment,
        "service-customer-piece",
        "Labor with customer piece",
        "service",
        "customer",
    )
    .expect("customer piece item");
    approve_item(
        &mut client,
        session,
        establishment,
        "service-customer-piece",
    )
    .expect("customer item reviewed");
    register_scenario(
        &mut client,
        session,
        establishment,
        "customer-piece-labor",
        "service",
        "customer",
        "itemized",
    )
    .expect("customer scenario");
    approve_scenario(
        &mut client,
        session,
        establishment,
        "customer-piece-labor",
        now - 10,
        None,
        "synthetic-review",
    )
    .expect("customer scenario approved");
    let mut customer_doc = documents[0].clone();
    customer_doc.item_ids = vec!["service-customer-piece".into()];
    save_decision_draft(
        &mut client,
        session,
        establishment,
        "customer-piece-labor",
        customer_fact_id,
        1,
        &[customer_doc],
    )
    .expect("customer decision draft");
    approve_decision(
        &mut client,
        session,
        DecisionApproval {
            establishment_id: establishment,
            scenario: "customer-piece-labor",
            fact_id: customer_fact_id,
            version: 1,
            effective_from_epoch: now - 10,
            effective_until_epoch: None,
            evidence_ref: "synthetic-review",
        },
    )
    .expect("customer decision approved");
    prepare_plan(
        session,
        &customer_fact,
        "customer-piece-labor",
        now + 10,
        &resolver,
    )
    .expect("labor with customer-owned piece");
    let mut mislabeled_customer_fact = customer_fact.clone();
    mislabeled_customer_fact["fact_id"] = Value::String(fact_id.to_string());
    assert!(matches!(
        prepare_plan(
            session,
            &mislabeled_customer_fact,
            "service-only",
            now + 10,
            &resolver
        ),
        Err(FiscalError::DecisionMissing)
    ));
    let mut workshop_fact = fixture("produto.json");
    let workshop_fact_id = Uuid::new_v4();
    workshop_fact["fact_id"] = Value::String(workshop_fact_id.to_string());
    let workshop_item = workshop_fact["items"][0]["item_id"]
        .as_str()
        .expect("product item")
        .to_owned();
    register_item(
        &mut client,
        session,
        establishment,
        &workshop_item,
        "Workshop-supplied product",
        "product",
        "workshop",
    )
    .expect("workshop item");
    approve_item(&mut client, session, establishment, &workshop_item)
        .expect("workshop item reviewed");
    register_scenario(
        &mut client,
        session,
        establishment,
        "goods-workshop",
        "goods",
        "workshop",
        "itemized",
    )
    .expect("workshop scenario");
    approve_scenario(
        &mut client,
        session,
        establishment,
        "goods-workshop",
        now - 10,
        None,
        "synthetic-review",
    )
    .expect("workshop scenario approved");
    let workshop_total = workshop_fact["totals"]["total_cents"]
        .as_i64()
        .expect("product total");
    save_decision_draft(
        &mut client,
        session,
        establishment,
        "goods-workshop",
        workshop_fact_id,
        1,
        &[DocumentPlan {
            key: "synthetic-goods".into(),
            kind: DocumentKind::Nfe,
            amount_cents: workshop_total,
            tax_base_cents: workshop_total,
            fiscal_code: "synthetic-goods-code".into(),
            item_ids: vec![workshop_item],
        }],
    )
    .expect("workshop decision draft");
    approve_decision(
        &mut client,
        session,
        DecisionApproval {
            establishment_id: establishment,
            scenario: "goods-workshop",
            fact_id: workshop_fact_id,
            version: 1,
            effective_from_epoch: now - 10,
            effective_until_epoch: None,
            evidence_ref: "synthetic-review",
        },
    )
    .expect("workshop decision approved");
    prepare_plan(
        session,
        &workshop_fact,
        "goods-workshop",
        now + 10,
        &resolver,
    )
    .expect("workshop-supplied goods");
    assert!(matches!(
        prepare_plan(
            session,
            &customer_fact,
            "goods-workshop",
            now + 10,
            &resolver
        ),
        Err(FiscalError::DecisionMissing)
    ));
    assert!(matches!(
        prepare_plan(session, &fact, "service-only", now + 30, &resolver),
        Err(FiscalError::DecisionMissing)
    ));
    let mut altered_fact = fact.clone();
    altered_fact["fact_id"] = Value::String(Uuid::new_v4().to_string());
    assert!(matches!(
        prepare_plan(session, &altered_fact, "service-only", now + 10, &resolver),
        Err(FiscalError::DecisionMissing)
    ));
    let mut product_fact = fixture("produto.json");
    product_fact["fact_id"] = Value::String(fact_id.to_string());
    assert!(matches!(
        prepare_plan(session, &product_fact, "service-only", now + 10, &resolver),
        Err(FiscalError::DecisionMissing)
    ));
    assert!(matches!(
        complete_issuer_profile(
            &mut client,
            session,
            id("00000000-0000-4000-8000-0000000000d5"),
            "0000000",
            "synthetic"
        ),
        Err(FiscalError::NotFound)
    ));

    let first = MasterKey::from_hex(&"01".repeat(32)).expect("key one");
    let second = MasterKey::from_hex(&"02".repeat(32)).expect("key two");
    save_provider_secret(
        &mut client,
        session,
        establishment,
        "synthetic-provider",
        "v1",
        &first,
        b"synthetic-credential",
    )
    .expect("save secret");
    assert!(read_provider_secret(
        &mut client,
        session,
        establishment,
        "synthetic-provider",
        &first,
        |secret| secret == b"synthetic-credential"
    )
    .expect("read inside service"));
    rotate_provider_secret(
        &mut client,
        session,
        establishment,
        "synthetic-provider",
        &first,
        &second,
        "v2",
    )
    .expect("rotate");
    assert!(read_provider_secret(
        &mut client,
        session,
        establishment,
        "synthetic-provider",
        &first,
        |_| true
    )
    .is_err());
    assert!(read_provider_secret(
        &mut client,
        session,
        establishment,
        "synthetic-provider",
        &second,
        |secret| secret == b"synthetic-credential"
    )
    .expect("read rotated"));
    let tenant_b = Session {
        company_id: id("00000000-0000-4000-8000-0000000000d1"),
        account_id: id("00000000-0000-4000-8000-0000000000d2"),
        product: Product::AutoBo,
        role: Role::Fiscal,
    };
    assert!(matches!(
        read_provider_secret(
            &mut client,
            tenant_b,
            establishment,
            "synthetic-provider",
            &second,
            |_| true
        ),
        Err(FiscalError::NotFound)
    ));
    drop(resolver);
    drop(client);
    db("postgres")
        .batch_execute("DROP DATABASE fiscal_config")
        .expect("drop ephemeral database");
}
