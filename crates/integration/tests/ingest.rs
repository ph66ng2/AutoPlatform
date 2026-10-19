use std::{env, fs, path::Path, thread};

use ap_identity::{Product, Role, Session};
use ap_integration::{
    find, find_by_correlation, find_for_fiscal, find_for_fiscal_by_correlation, ingest, IngestError,
};
use postgres::{Client, NoTls};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn db(name: &str) -> Client {
    let mut config = postgres::Config::new();
    config
        .host(&env::var("PGHOST").expect("PGHOST"))
        .port(env::var("PGPORT").expect("PGPORT").parse().expect("port"))
        .user(&env::var("PGUSER").expect("PGUSER"))
        .password(env::var("PGPASSWORD").expect("PGPASSWORD"))
        .dbname(name);
    config.connect(NoTls).expect("postgres")
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("uuid")
}
fn fixture() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v2/examples/valid/servico.json");
    serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("json")
}

#[test]
fn tenant_idempotency_conflict_concurrent_and_recovery() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let mut admin = db("postgres");
    admin
        .batch_execute("DROP DATABASE IF EXISTS integration_facts")
        .expect("drop");
    admin
        .batch_execute("CREATE DATABASE integration_facts")
        .expect("create");
    drop(admin);
    let mut client = db("integration_facts");
    for file in [
        "0001_baseline.sql",
        "0002_identity.sql",
        "0003_inventory.sql",
        "0004_fiscal.sql",
        "0005_fiscal_operation_recovery.sql",
        "0006_commercial_fact_ingest.sql",
    ] {
        let sql = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .join(file),
        )
        .expect("sql");
        client.batch_execute(&sql).expect("migration");
    }
    client.batch_execute("
      INSERT INTO identity.companies(id,label,status) VALUES
      ('00000000-0000-4000-8000-0000000000a1','a','active'),
      ('00000000-0000-4000-8000-0000000000d1','b','active');
      INSERT INTO identity.accounts(id,status) VALUES
      ('00000000-0000-4000-8000-0000000000a2','active'),
      ('00000000-0000-4000-8000-0000000000d2','active');
      INSERT INTO identity.memberships(company_id,id,account_id,product,role,status) VALUES
      ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a3','00000000-0000-4000-8000-0000000000a2','autoos','operator','active'),
      ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d3','00000000-0000-4000-8000-0000000000d2','autoos','operator','active'),
      ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a4','00000000-0000-4000-8000-0000000000a2','autobo','fiscal','active'),
      ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d4','00000000-0000-4000-8000-0000000000d2','autobo','fiscal','active');
    ").expect("seed");
    let company = id("00000000-0000-4000-8000-0000000000a1");
    let account = id("00000000-0000-4000-8000-0000000000a2");
    let other_company = id("00000000-0000-4000-8000-0000000000d1");
    let other_account = id("00000000-0000-4000-8000-0000000000d2");
    let fact = fixture();
    let fact_id = id(fact["fact_id"].as_str().expect("fact id"));
    // Snapshot antigo: a migration 0006 criou as colunas, mas elas ainda são NULL.
    let mut legacy = fact.clone();
    legacy["fact_id"] = Uuid::new_v4().to_string().into();
    legacy["event_id"] = Uuid::new_v4().to_string().into();
    legacy["os_id"] = Uuid::new_v4().to_string().into();
    legacy["idempotency_key"] = "idem-legacy-synthetic-0001".into();
    legacy["correlation_id"] = "corr-legacy-synthetic-0001".into();
    let legacy_json = legacy.to_string();
    let legacy_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&legacy).expect("json"))
    );
    client.execute(
        "INSERT INTO fiscal.fact_snapshots(company_id,fact_id,os_id,os_version,origin,payload_hash,payload) VALUES($1,$2,$3,1,'autoos',$4,($5::text)::jsonb)",
        &[&company, &id(legacy["fact_id"].as_str().expect("fact")), &id(legacy["os_id"].as_str().expect("os")), &legacy_hash, &legacy_json],
    ).expect("legacy snapshot");
    let migration = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations/0007_commercial_fact_keys.sql"),
    )
    .expect("migration 0007");
    client.batch_execute(&migration).expect("backfill");
    assert!(
        !ingest(&mut client, company, account, &legacy)
            .expect("legacy retry")
            .created
    );
    assert_eq!(
        find_by_correlation(&mut client, company, account, "corr-legacy-synthetic-0001")
            .expect("legacy correlation"),
        vec![legacy]
    );
    // O caminho fiscal legado omite as colunas; o trigger preenche a partir do payload.
    let mut later = fact.clone();
    later["fact_id"] = Uuid::new_v4().to_string().into();
    later["event_id"] = Uuid::new_v4().to_string().into();
    later["os_id"] = Uuid::new_v4().to_string().into();
    later["idempotency_key"] = "idem-later-synthetic-0001".into();
    later["correlation_id"] = "corr-later-synthetic-0001".into();
    let later_json = later.to_string();
    let later_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&later).expect("json"))
    );
    client.execute(
        "INSERT INTO fiscal.fact_snapshots(company_id,fact_id,os_id,os_version,origin,payload_hash,payload) VALUES($1,$2,$3,1,'autoos',$4,($5::text)::jsonb)",
        &[&company, &id(later["fact_id"].as_str().expect("fact")), &id(later["os_id"].as_str().expect("os")), &later_hash, &later_json],
    ).expect("legacy insertion");
    assert!(
        !ingest(&mut client, company, account, &later)
            .expect("later retry")
            .created
    );
    let mut invalid = fact.clone();
    invalid.as_object_mut().expect("object").remove("items");
    assert_eq!(
        ingest(&mut client, company, account, &invalid),
        Err(IngestError::Invalid)
    );
    assert_eq!(
        ingest(&mut client, company, other_account, &fact),
        Err(IngestError::Denied)
    );
    assert_eq!(
        ingest(&mut client, other_company, other_account, &fact),
        Err(IngestError::Denied)
    );
    let first = ingest(&mut client, company, account, &fact).expect("first");
    assert!(first.created);
    assert_eq!(first.fact_id, fact_id);
    assert!(
        !ingest(&mut client, company, account, &fact)
            .expect("retry")
            .created
    );
    assert_eq!(
        find(&mut client, company, account, fact_id).expect("read"),
        fact
    );
    assert_eq!(
        find(&mut client, other_company, other_account, fact_id),
        Err(IngestError::NotFound)
    );
    let correlation = fact["correlation_id"].as_str().expect("correlation");
    let fiscal = Session {
        company_id: company,
        account_id: account,
        product: Product::AutoBo,
        role: Role::Fiscal,
    };
    let other_fiscal = Session {
        company_id: other_company,
        account_id: other_account,
        product: Product::AutoBo,
        role: Role::Fiscal,
    };
    assert_eq!(
        find_for_fiscal(&mut client, fiscal, fact_id).expect("fiscal id"),
        fact
    );
    assert_eq!(
        find_for_fiscal(&mut client, other_fiscal, fact_id),
        Err(IngestError::NotFound)
    );
    assert_eq!(
        find_for_fiscal_by_correlation(&mut client, fiscal, correlation)
            .expect("fiscal correlation"),
        vec![fact.clone()]
    );
    assert_eq!(
        find_by_correlation(&mut client, company, account, correlation).expect("correlation"),
        vec![fact.clone()]
    );
    assert!(
        find_by_correlation(&mut client, other_company, other_account, correlation)
            .expect("isolated")
            .is_empty()
    );
    let mut changed = fact.clone();
    changed["customer"]["name"] = "Outro cliente".into();
    assert_eq!(
        ingest(&mut client, company, account, &changed),
        Err(IngestError::Conflict)
    );
    let mut reused_key = fact.clone();
    reused_key["fact_id"] = Uuid::new_v4().to_string().into();
    reused_key["os_id"] = Uuid::new_v4().to_string().into();
    reused_key["event_id"] = Uuid::new_v4().to_string().into();
    assert_eq!(
        ingest(&mut client, company, account, &reused_key),
        Err(IngestError::Conflict)
    );
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let fact = fact.clone();
            thread::spawn(move || {
                let mut client = db("integration_facts");
                ingest(&mut client, company, account, &fact)
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(
            handle
                .join()
                .expect("thread")
                .expect("retry concurrent")
                .fact_id,
            fact_id
        );
    }
    client.batch_execute("UPDATE identity.memberships SET status='revoked' WHERE company_id='00000000-0000-4000-8000-0000000000a1'").expect("revoke");
    assert_eq!(
        find(&mut client, company, account, fact_id),
        Err(IngestError::Denied)
    );
}
