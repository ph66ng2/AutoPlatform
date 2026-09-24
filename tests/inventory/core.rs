//! Núcleo de estoque contra PostgreSQL descartável. Sem dado operacional.

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
};

use ap_inventory::{
    availability, commit_reservation, consume, expire_reservation, receive, reconcile,
    register_product, release_reservation, reserve, InventoryError,
};
use postgres::{Client, NoTls};
use serde_json::json;
use uuid::Uuid;

#[test]
fn ledger_reservation_cycle_concurrency_and_tenants() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let _cleanup = ClusterGuard;
    let mut admin = connect("postgres");
    reset_cluster(&mut admin);
    admin
        .batch_execute("CREATE DATABASE inventory_core")
        .expect("cria banco");
    drop(admin);

    let mut db = connect("inventory_core");
    for file in [
        "0001_baseline.sql",
        "0002_identity.sql",
        "0003_inventory.sql",
    ] {
        let sql = fs::read_to_string(migration(file)).unwrap_or_else(|err| panic!("{file}: {err}"));
        db.batch_execute(&sql)
            .unwrap_or_else(|err| panic!("{file}: {err}"));
    }
    seed(&mut db);
    assert_policies_have_using_and_check(&mut db);
    assert_app_cannot_edit_ledger(&mut db);

    assume(&mut db, company_a(), account_a());
    register_product(&mut db, "FILTRO-OLEO-001").expect("produto");
    let first = receive(&mut db, "FILTRO-OLEO-001", 1, "idem-receive-filtro01").expect("entrada");
    let again = receive(&mut db, "FILTRO-OLEO-001", 1, "idem-receive-filtro01").expect("replay");
    assert_eq!(first.movement_id, again.movement_id);
    assert_eq!(first.on_hand, 1);
    assert_eq!(again.on_hand, 1);
    let stock = availability(&mut db, "FILTRO-OLEO-001").expect("saldo");
    assert_eq!(stock.on_hand, 1);
    assert_eq!(stock.reserved, 0);
    assert_eq!(stock.available, 1);

    let reserved = reserve(
        &mut db,
        "FILTRO-OLEO-001",
        1,
        "idem-reserve-filtro01",
        "15 minutes",
    )
    .expect("reserva");
    assert_eq!(reserved.reason, "reserve");
    assert_eq!(reserved.quantity, -1);
    assert_eq!(reserved.on_hand, 1);
    assert_eq!(reserved.reserved, 1);
    assert_eq!(reserved.available, 0);
    assert_eq!(
        reserve(
            &mut db,
            "FILTRO-OLEO-001",
            1,
            "idem-reserve-filtro01",
            "15 minutes",
        )
        .expect("reserva repetida")
        .reservation_id,
        reserved.reservation_id
    );
    assert_eq!(
        consume(&mut db, "FILTRO-OLEO-001", 1, "idem-consume-blocked01"),
        Err(InventoryError::Insufficient)
    );
    let after_reserve = availability(&mut db, "FILTRO-OLEO-001").expect("depois da reserva");
    assert_eq!(after_reserve.on_hand, 1);
    assert_eq!(after_reserve.available, 0);

    let committed = commit_reservation(&mut db, "idem-reserve-filtro01", "idem-commit-filtro01")
        .expect("commit");
    assert_eq!(committed.reason, "consume");
    assert_eq!(committed.on_hand, 0);
    assert_eq!(committed.reserved, 0);
    assert_eq!(
        commit_reservation(&mut db, "idem-reserve-filtro01", "idem-commit-filtro01")
            .expect("commit repetido")
            .movement_id,
        committed.movement_id
    );

    receive(&mut db, "FILTRO-OLEO-001", 2, "idem-receive-filtro02").expect("reposição");
    reserve(
        &mut db,
        "FILTRO-OLEO-001",
        1,
        "idem-reserve-release01",
        "15 minutes",
    )
    .expect("reserva para release");
    let released = release_reservation(&mut db, "idem-reserve-release01", "idem-release-filtro01")
        .expect("release");
    assert_eq!(released.reason, "release");
    assert_eq!(released.on_hand, 2);
    assert_eq!(released.reserved, 0);
    assert_eq!(released.available, 2);

    reserve(
        &mut db,
        "FILTRO-OLEO-001",
        1,
        "idem-reserve-expire01",
        "-1 second",
    )
    .expect("reserva vencida");
    let expired = expire_reservation(&mut db, "idem-reserve-expire01", "idem-expire-filtro01")
        .expect("expira");
    assert_eq!(expired.reason, "release");
    assert_eq!(
        availability(&mut db, "FILTRO-OLEO-001")
            .expect("expirada")
            .available,
        2
    );
    assert_eq!(reconcile(&mut db).expect("reconcilia"), 0);

    consume(&mut db, "FILTRO-OLEO-001", 1, "idem-consume-filtro01").expect("saída");
    let correction =
        receive(&mut db, "FILTRO-OLEO-001", 1, "idem-receive-correct01").expect("correção");
    assert_eq!(correction.reason, "receive");
    assert_eq!(
        availability(&mut db, "FILTRO-OLEO-001")
            .expect("final")
            .on_hand,
        2
    );

    db.batch_execute("RESET ROLE").expect("reset");
    db.execute(
        "SELECT set_config('app.inventory_mode', 'offline', false)",
        &[],
    )
    .expect("offline");
    assume(&mut db, company_a(), account_a());
    assert_eq!(
        receive(&mut db, "FILTRO-OLEO-001", 1, "idem-receive-offline1"),
        Err(InventoryError::Offline)
    );
    db.batch_execute("RESET ROLE").expect("reset");
    db.execute(
        "SELECT set_config('app.inventory_mode', 'online', false)",
        &[],
    )
    .expect("online");

    assume(&mut db, company_b(), account_b());
    assert_eq!(
        availability(&mut db, "FILTRO-OLEO-001"),
        Err(InventoryError::NotFound)
    );
    register_product(&mut db, "FILTRO-OLEO-001").expect("produto b");
    receive(&mut db, "FILTRO-OLEO-001", 8, "idem-receive-tenant-b1").expect("entrada b");
    assert_eq!(
        availability(&mut db, "FILTRO-OLEO-001")
            .expect("saldo b")
            .on_hand,
        8
    );
    assume(&mut db, company_a(), account_a());
    assert_eq!(
        availability(&mut db, "FILTRO-OLEO-001")
            .expect("saldo a isolado")
            .on_hand,
        2
    );
    let leaked: i64 = db
        .query_one(
            "SELECT count(*) FROM inventory.movements WHERE company_id = $1",
            &[&company_b()],
        )
        .expect("cruzado")
        .get(0);
    assert_eq!(leaked, 0);

    let event = json!({
        "schema_version": "1",
        "company_id": "00000000-0000-4000-8000-0000000000a1",
        "correlation_id": "corr-estoque-movimento",
        "event_id": "event-estoque-movimen",
        "idempotency_key": "idem-reserve-filtro01",
        "movement_id": reserved.reservation_id.as_ref().expect("reserva"),
        "sku": "FILTRO-OLEO-001",
        "quantity": -1,
        "reason": "reserve",
        "occurred_at": "2026-09-23T12:00:02Z"
    });
    let fixture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/v1/fixtures/valid/estoque.json"),
        )
        .expect("fixture"),
    )
    .expect("json");
    assert_eq!(event["schema_version"], fixture["schema_version"]);
    assert_eq!(event["reason"], fixture["reason"]);
    assert_eq!(event["quantity"], fixture["quantity"]);
    assert_eq!(event["sku"], fixture["sku"]);

    last_unit_is_serialized();
}

fn last_unit_is_serialized() {
    receive_on(
        &mut connect("inventory_core"),
        "PARAFUSO-UNICO-01",
        1,
        "idem-receive-lastunit1",
    )
    .expect("última unidade");
    let barrier = Arc::new(Barrier::new(2));
    let mut joins = Vec::new();
    for index in 0..2 {
        let barrier = Arc::clone(&barrier);
        joins.push(thread::spawn(move || {
            let mut db = connect("inventory_core");
            assume(&mut db, company_a(), account_a());
            barrier.wait();
            reserve(
                &mut db,
                "PARAFUSO-UNICO-01",
                1,
                &format!("idem-reserve-lastunit{index}"),
                "15 minutes",
            )
        }));
    }
    let results: Vec<Result<_, _>> = joins
        .into_iter()
        .map(|join| join.join().expect("thread"))
        .collect();
    let wins = results.iter().filter(|item| item.is_ok()).count();
    let losses = results
        .iter()
        .filter(|item| *item == &Err(InventoryError::Insufficient))
        .count();
    assert_eq!(wins, 1);
    assert_eq!(losses, 1);
}

fn receive_on(
    db: &mut Client,
    sku: &str,
    quantity: i32,
    operation_id: &str,
) -> Result<ap_inventory::CommandOutcome, InventoryError> {
    assume(db, company_a(), account_a());
    register_product(db, sku)?;
    receive(db, sku, quantity, operation_id)
}

fn assert_app_cannot_edit_ledger(db: &mut Client) {
    assume(db, company_a(), account_a());
    let insert = db.execute(
        "INSERT INTO inventory.movements (company_id, id, product_id, warehouse_id, quantity, reason, operation_id) VALUES ($1, $2, $2, $2, 1, 'receive', 'idem-direct-ledger01')",
        &[&company_a(), &Uuid::from_u128(1)],
    );
    assert_eq!(
        insert
            .expect_err("insert direto")
            .code()
            .map(|code| code.code()),
        Some("42501")
    );
}

fn assert_policies_have_using_and_check(db: &mut Client) {
    let rows = db
        .query(
            "
            SELECT c.relname, pg_get_expr(p.polqual, p.polrelid), pg_get_expr(p.polwithcheck, p.polrelid)
            FROM pg_policy p
            JOIN pg_class c ON c.oid = p.polrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'inventory'
            ",
            &[],
        )
        .expect("policies");
    assert!(rows.len() >= 6);
    for row in rows {
        let table: String = row.get(0);
        let using = row
            .get::<_, Option<String>>(1)
            .unwrap_or_else(|| panic!("{table} sem USING"));
        let check = row
            .get::<_, Option<String>>(2)
            .unwrap_or_else(|| panic!("{table} sem WITH CHECK"));
        assert!(using.contains("current_company_id"), "{table}");
        assert!(check.contains("current_company_id"), "{table}");
    }
}

fn company_a() -> Uuid {
    id("00000000-0000-4000-8000-0000000000a1")
}

fn company_b() -> Uuid {
    id("00000000-0000-4000-8000-0000000000b1")
}

fn account_a() -> Uuid {
    id("00000000-0000-4000-8000-0000000000a2")
}

fn account_b() -> Uuid {
    id("00000000-0000-4000-8000-0000000000b2")
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("uuid")
}

fn migration(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .join(name)
}

fn connect(database: &str) -> Client {
    postgres::Config::new()
        .host(&env::var("PGHOST").expect("PGHOST"))
        .port(
            env::var("PGPORT")
                .ok()
                .and_then(|port| port.parse().ok())
                .unwrap_or(5432),
        )
        .user(&env::var("PGUSER").expect("PGUSER"))
        .password(env::var("PGPASSWORD").expect("PGPASSWORD"))
        .dbname(database)
        .connect(NoTls)
        .expect("conexão")
}

fn reset_cluster(admin: &mut Client) {
    admin
        .batch_execute(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'inventory_core' AND pid <> pg_backend_pid()",
        )
        .expect("encerra sessões");
    admin
        .batch_execute("DROP DATABASE IF EXISTS inventory_core")
        .expect("remove banco");
}

fn seed(db: &mut Client) {
    db.batch_execute(
        "
        INSERT INTO identity.companies (id, label, status) VALUES
            ('00000000-0000-4000-8000-0000000000a1', 'tenant-a', 'active'),
            ('00000000-0000-4000-8000-0000000000b1', 'tenant-b', 'active');
        INSERT INTO identity.accounts (id, status) VALUES
            ('00000000-0000-4000-8000-0000000000a2', 'active'),
            ('00000000-0000-4000-8000-0000000000b2', 'active');
        INSERT INTO identity.memberships (company_id, id, account_id, product, role, status) VALUES
            ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000a3', '00000000-0000-4000-8000-0000000000a2', 'autobo', 'admin', 'active'),
            ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b3', '00000000-0000-4000-8000-0000000000b2', 'autobo', 'admin', 'active');
        ",
    )
    .expect("fixtures");
}

fn assume(db: &mut Client, company: Uuid, account: Uuid) {
    db.batch_execute("RESET ROLE").expect("reset");
    db.execute(
        "SELECT set_config('app.company_id', $1, false), set_config('app.account_id', $2, false)",
        &[&company.to_string(), &account.to_string()],
    )
    .expect("contexto");
    db.batch_execute("SET ROLE inventory_app").expect("papel");
}

struct ClusterGuard;

impl Drop for ClusterGuard {
    fn drop(&mut self) {
        let mut config = postgres::Config::new();
        if let (Ok(host), Ok(user), Ok(password)) = (
            env::var("PGHOST"),
            env::var("PGUSER"),
            env::var("PGPASSWORD"),
        ) {
            config
                .host(&host)
                .port(
                    env::var("PGPORT")
                        .ok()
                        .and_then(|port| port.parse().ok())
                        .unwrap_or(5432),
                )
                .user(&user)
                .password(password)
                .dbname("postgres");
            if let Ok(mut admin) = config.connect(NoTls) {
                reset_cluster(&mut admin);
            }
        }
    }
}
