//! Gate contratual com clientes falsos e PostgreSQL descartável. Sem provider real.

use std::{env, fs, path::Path};

use ap_identity::{Product, Role, Session};
use ap_integration::{find, find_by_correlation, find_for_fiscal, ingest, IngestError};
use ap_inventory::{availability, receive, register_product, reserve, InventoryError};
use postgres::{Client, NoTls};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DATABASE: &str = "integration_gate_fixtures";
const SKU: &str = "SYNC-PIECE-001";
const COMPANY_A: &str = "00000000-0000-4000-8000-0000000000a1";
const ACCOUNT_A: &str = "00000000-0000-4000-8000-0000000000a2";
const COMPANY_B: &str = "00000000-0000-4000-8000-0000000000d1";
const ACCOUNT_B: &str = "00000000-0000-4000-8000-0000000000d2";

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("synthetic UUID")
}

fn connection_config(database: &str) -> postgres::Config {
    let mut config = postgres::Config::new();
    config
        .host(&env::var("PGHOST").expect("PGHOST"))
        .port(env::var("PGPORT").expect("PGPORT").parse().expect("port"))
        .user(&env::var("PGUSER").expect("PGUSER"))
        .password(env::var("PGPASSWORD").expect("PGPASSWORD"))
        .dbname(database);
    config
}

fn connect(database: &str) -> Client {
    connection_config(database)
        .connect(NoTls)
        .expect("fixture postgres")
}

fn reset_database() {
    let mut admin = connect("postgres");
    let exists: bool = admin
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)",
            &[&DATABASE],
        )
        .expect("fixture database lookup")
        .get(0);
    if exists {
        admin
            .batch_execute(&format!("ALTER DATABASE {DATABASE} ALLOW_CONNECTIONS true"))
            .expect("restore fixture connections");
    }
    admin
        .query(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname=$1 AND pid<>pg_backend_pid()",
            &[&DATABASE],
        )
        .expect("terminate fixture sessions");
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {DATABASE}"))
        .expect("drop fixture database");
}

struct FixtureGuard;

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        reset_database();
    }
}

fn setup() -> (FixtureGuard, Client) {
    reset_database();
    connect("postgres")
        .batch_execute(&format!("CREATE DATABASE {DATABASE}"))
        .expect("create fixture database");
    let guard = FixtureGuard;
    let mut db = connect(DATABASE);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in [
        "0001_baseline.sql",
        "0002_identity.sql",
        "0003_inventory.sql",
        "0004_fiscal.sql",
        "0005_fiscal_operation_recovery.sql",
        "0006_commercial_fact_ingest.sql",
        "0007_commercial_fact_keys.sql",
    ] {
        db.batch_execute(&fs::read_to_string(root.join("migrations").join(name)).expect(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    db.batch_execute(
        "INSERT INTO identity.companies(id,label,status) VALUES
         ('00000000-0000-4000-8000-0000000000a1','synthetic-a','active'),
         ('00000000-0000-4000-8000-0000000000d1','synthetic-b','active');
         INSERT INTO identity.accounts(id,status) VALUES
         ('00000000-0000-4000-8000-0000000000a2','active'),
         ('00000000-0000-4000-8000-0000000000d2','active');
         INSERT INTO identity.memberships(company_id,id,account_id,product,role,status) VALUES
         ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a3','00000000-0000-4000-8000-0000000000a2','autoos','operator','active'),
         ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a4','00000000-0000-4000-8000-0000000000a2','autobo','fiscal','active'),
         ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d3','00000000-0000-4000-8000-0000000000d2','autoos','operator','active'),
         ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d4','00000000-0000-4000-8000-0000000000d2','autobo','fiscal','active');",
    )
    .expect("synthetic identity");
    (guard, db)
}

fn inventory_as(db: &mut Client, company: &str, account: &str) {
    db.batch_execute("RESET ROLE").expect("reset role");
    db.execute(
        "SELECT set_config('app.company_id',$1,false),set_config('app.account_id',$2,false)",
        &[&company, &account],
    )
    .expect("tenant context");
    db.batch_execute("SET ROLE inventory_app")
        .expect("inventory role");
}

fn fact_for(company: &str, suffix: u128) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut fact: Value = serde_json::from_str(
        &fs::read_to_string(root.join("contracts/v2/examples/valid/servico.json"))
            .expect("synthetic example"),
    )
    .expect("fact JSON");
    fact["company_id"] = company.into();
    fact["fact_id"] = Uuid::from_u128(suffix + 0x100).to_string().into();
    fact["event_id"] = Uuid::from_u128(suffix + 0x200).to_string().into();
    fact["os_id"] = Uuid::from_u128(suffix + 0x300).to_string().into();
    fact["idempotency_key"] = format!("idem-gate-synthetic-{suffix:016x}").into();
    fact["correlation_id"] = format!("corr-gate-synthetic-{suffix:016x}").into();
    fact
}

fn fiscal_session(company: &str, account: &str) -> Session {
    Session {
        company_id: id(company),
        account_id: id(account),
        product: Product::AutoBo,
        role: Role::Fiscal,
    }
}

fn assert_v1_contract(root: &Path, name: &str, body: &Value) {
    let path = root.join(format!("contracts/v1/{name}.schema.json"));
    let bytes = fs::read(&path).expect("v1 schema");
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let manifest =
        fs::read_to_string(root.join("contracts/v1/manifest.sha256")).expect("v1 manifest");
    assert!(manifest.contains(&format!("{hash}  {name}.schema.json")));
    let schema: Value = serde_json::from_slice(&bytes).expect("v1 schema JSON");
    let properties = schema["properties"].as_object().expect("properties");
    for key in schema["required"].as_array().expect("required") {
        assert!(body.get(key.as_str().expect("required key")).is_some());
    }
    for (key, value) in body.as_object().expect("contract body") {
        let property = properties.get(key).expect("known field");
        if let Some(constant) = property.get("const") {
            assert_eq!(value, constant);
        }
        if let Some(variants) = property.get("enum") {
            assert!(variants.as_array().expect("enum").contains(value));
        }
        if property.get("format") == Some(&Value::String("uuid".into())) {
            assert!(Uuid::parse_str(value.as_str().expect("uuid string")).is_ok());
        }
    }
}

#[test]
fn two_tenant_journey_retry_and_unavailable() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let (_guard, mut db) = setup();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let v2_schema = root.join("contracts/v2/fato-comercial.schema.json");
    let v2_hash = format!(
        "{:x}",
        Sha256::digest(fs::read(&v2_schema).expect("v2 schema"))
    );
    let manifest =
        fs::read_to_string(root.join("contracts/v2/manifest.sha256")).expect("v2 manifest");
    assert!(manifest.contains(&format!("{v2_hash}  fato-comercial.schema.json")));

    let fact_a = fact_for(COMPANY_A, 0xa1);
    let fact_b = fact_for(COMPANY_B, 0xd1);
    let fact_a_id = id(fact_a["fact_id"].as_str().expect("fact id"));
    let correlation = fact_a["correlation_id"].as_str().expect("correlation");

    // Fake AutoOS: entrega, perde a resposta e consulta antes do retry.
    let first = ingest(&mut db, id(COMPANY_A), id(ACCOUNT_A), &fact_a).expect("publish A");
    assert!(first.created);
    assert_eq!(first.fact_id, fact_a_id);
    assert_eq!(
        find_by_correlation(&mut db, id(COMPANY_A), id(ACCOUNT_A), correlation)
            .expect("recover lost response"),
        vec![fact_a.clone()]
    );
    assert!(
        !ingest(&mut db, id(COMPANY_A), id(ACCOUNT_A), &fact_a)
            .expect("safe retry")
            .created
    );
    assert!(
        ingest(&mut db, id(COMPANY_B), id(ACCOUNT_B), &fact_b)
            .expect("publish B")
            .created
    );
    assert_eq!(
        find(&mut db, id(COMPANY_B), id(ACCOUNT_B), fact_a_id),
        Err(IngestError::NotFound)
    );
    assert_eq!(
        find_by_correlation(&mut db, id(COMPANY_B), id(ACCOUNT_B), correlation)
            .expect("isolated correlation"),
        Vec::<Value>::new()
    );
    let snapshots: i64 = db
        .query_one("SELECT count(*) FROM fiscal.fact_snapshots", &[])
        .expect("snapshot count")
        .get(0);
    assert_eq!(snapshots, 2);

    // Fake AutoBO: uma intenção local de revisão, pendente, sem presumir pagamento.
    let consumed = find_for_fiscal(&mut db, fiscal_session(COMPANY_A, ACCOUNT_A), fact_a_id)
        .expect("fiscal projection");
    assert_eq!(consumed, fact_a);
    assert_eq!(
        find_for_fiscal(&mut db, fiscal_session(COMPANY_B, ACCOUNT_B), fact_a_id),
        Err(IngestError::NotFound)
    );
    let state = json!({
        "schema_version": "1", "company_id": COMPANY_A,
        "correlation_id": correlation, "event_id": "event-gate-state-synthetic-0001",
        "idempotency_key": "idem-gate-state-synthetic-0001",
        "fact_id": fact_a_id.to_string(), "state": "pending",
        "amount_cents": consumed["totals"]["total_cents"],
        "occurred_at": "2026-10-19T12:00:00Z"
    });
    assert_v1_contract(&root, "estado-financeiro", &state);
    assert_eq!(state["state"], "pending");
    assert_eq!(state["correlation_id"], correlation);

    // Comandos reais de estoque continuam isolados e idempotentes.
    inventory_as(&mut db, COMPANY_A, ACCOUNT_A);
    register_product(&mut db, SKU).expect("register A");
    receive(&mut db, SKU, 2, "idem-gate-receive-a-0001").expect("receive A");
    let reserved =
        reserve(&mut db, SKU, 1, "idem-gate-reserve-a-0001", "15 minutes").expect("reserve A");
    assert_eq!(reserved.available, 1);
    assert_eq!(
        reserve(&mut db, SKU, 1, "idem-gate-reserve-a-0001", "15 minutes")
            .expect("reserve retry")
            .reservation_id,
        reserved.reservation_id
    );
    assert_eq!(availability(&mut db, SKU).expect("stock A").reserved, 1);
    let stock_event = json!({
        "schema_version": "1", "company_id": COMPANY_A,
        "correlation_id": correlation, "event_id": "event-gate-stock-synthetic-0001",
        "idempotency_key": "idem-gate-reserve-a-0001",
        "movement_id": reserved.reservation_id.as_ref().expect("reservation id"),
        "sku": SKU, "quantity": -1, "reason": "reserve",
        "occurred_at": "2026-10-19T12:00:01Z"
    });
    assert_v1_contract(&root, "estoque", &stock_event);
    assert_eq!(stock_event["correlation_id"], state["correlation_id"]);
    let reservation_commands: i64 = db
        .query_one(
            "SELECT count(*) FROM inventory.commands WHERE operation_id=$1",
            &[&"idem-gate-reserve-a-0001"],
        )
        .expect("one reservation command")
        .get(0);
    assert_eq!(reservation_commands, 1);
    inventory_as(&mut db, COMPANY_B, ACCOUNT_B);
    assert_eq!(availability(&mut db, SKU), Err(InventoryError::NotFound));
    register_product(&mut db, SKU).expect("register B");
    receive(&mut db, SKU, 5, "idem-gate-receive-b-0001").expect("receive B");
    assert_eq!(availability(&mut db, SKU).expect("stock B").available, 5);
    inventory_as(&mut db, COMPANY_A, ACCOUNT_A);
    assert_eq!(
        availability(&mut db, SKU).expect("stock A again").available,
        1
    );

    // O próprio banco de fixtures fica indisponível. Encerramos a sessão
    // ativa, recusamos novas conexões e só então reabrimos e reconectamos.
    let mut admin = connect("postgres");
    admin
        .batch_execute(&format!(
            "ALTER DATABASE {DATABASE} ALLOW_CONNECTIONS false"
        ))
        .expect("disable fixture database");
    admin
        .query(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname=$1 AND pid<>pg_backend_pid()",
            &[&DATABASE],
        )
        .expect("terminate fixture connection");
    assert!(db.query_one("SELECT 1", &[]).is_err());
    assert!(connection_config(DATABASE).connect(NoTls).is_err());
    admin
        .batch_execute(&format!("ALTER DATABASE {DATABASE} ALLOW_CONNECTIONS true"))
        .expect("restore fixture database");
    drop(db);
    let mut db = connect(DATABASE);
    assert_eq!(
        find_by_correlation(&mut db, id(COMPANY_A), id(ACCOUNT_A), correlation)
            .expect("recovered lookup")
            .len(),
        1
    );
    inventory_as(&mut db, COMPANY_A, ACCOUNT_A);
    assert_eq!(
        availability(&mut db, SKU)
            .expect("stock after outage")
            .available,
        1
    );
    db.batch_execute("RESET ROLE").expect("reset role");
    let final_count: i64 = db
        .query_one("SELECT count(*) FROM fiscal.fact_snapshots", &[])
        .expect("final snapshots")
        .get(0);
    assert_eq!(final_count, 2);
}
