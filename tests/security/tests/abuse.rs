//! Abuso A/B no Postgres descartável. Sem dado real.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use postgres::{Client, NoTls};
use uuid::Uuid;

#[test]
fn rls_constraints_and_cross_writes_fail_closed() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let _cleanup = ClusterGuard;
    let mut admin = connect("postgres");
    reset_cluster(&mut admin);
    admin
        .batch_execute("CREATE DATABASE sec_gate_rls")
        .expect("cria banco");
    drop(admin);

    let mut db = connect("sec_gate_rls");
    for file in ["0001_baseline.sql", "0002_identity.sql"] {
        let sql = fs::read_to_string(migration(file)).unwrap_or_else(|err| panic!("{file}: {err}"));
        db.batch_execute(&sql)
            .unwrap_or_else(|err| panic!("{file}: {err}"));
    }
    seed(&mut db);

    let rolbypass: bool = db
        .query_one(
            "SELECT rolbypassrls FROM pg_roles WHERE rolname = 'identity_app'",
            &[],
        )
        .expect("papel")
        .get(0);
    assert!(!rolbypass);
    let forced: i64 = db
        .query_one(
            "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'identity' AND c.relrowsecurity AND c.relforcerowsecurity",
            &[],
        )
        .expect("force")
        .get(0);
    assert!(forced >= 4);

    let autoos_fiscal = db.execute(
        "INSERT INTO identity.memberships (company_id, id, account_id, product, role, status) VALUES ($1, $2, $3, 'autoos', 'fiscal', 'active')",
        &[
            &id("00000000-0000-4000-8000-0000000000a1"),
            &id("00000000-0000-4000-8000-0000000000a9"),
            &id("00000000-0000-4000-8000-0000000000a2"),
        ],
    );
    assert!(autoos_fiscal.is_err());

    assume(
        &mut db,
        id("00000000-0000-4000-8000-0000000000a1"),
        id("00000000-0000-4000-8000-0000000000a2"),
    );
    let foreign: i64 = db
        .query_one(
            "SELECT count(*) FROM identity.memberships WHERE company_id = $1",
            &[&id("00000000-0000-4000-8000-0000000000b1")],
        )
        .expect("cruzado")
        .get(0);
    assert_eq!(foreign, 0);
    let write_membership = db.execute(
        "INSERT INTO identity.memberships (company_id, id, account_id, product, role, status) VALUES ($1, $2, $3, 'autobo', 'fiscal', 'active')",
        &[
            &id("00000000-0000-4000-8000-0000000000b1"),
            &id("00000000-0000-4000-8000-0000000000aa"),
            &id("00000000-0000-4000-8000-0000000000a2"),
        ],
    );
    assert_eq!(
        write_membership
            .expect_err("membership alheia")
            .code()
            .map(|code| code.code()),
        Some("42501")
    );

    db.batch_execute("RESET ROLE").expect("reset");
    db.execute(
        "SELECT set_config('app.company_id', $1, false), set_config('app.account_id', $2, false)",
        &[
            &id("00000000-0000-4000-8000-0000000000b1").to_string(),
            &id("00000000-0000-4000-8000-0000000000a2").to_string(),
        ],
    )
    .expect("contexto invertido");
    db.batch_execute("SET ROLE identity_app").expect("papel");
    let inverted: i64 = db
        .query_one("SELECT count(*) FROM identity.companies", &[])
        .expect("invertido")
        .get(0);
    assert_eq!(inverted, 0);
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
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'sec_gate_rls' AND pid <> pg_backend_pid()",
        )
        .expect("encerra");
    admin
        .batch_execute("DROP DATABASE IF EXISTS sec_gate_rls")
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
            ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000a3', '00000000-0000-4000-8000-0000000000a2', 'autoos', 'admin', 'active'),
            ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b3', '00000000-0000-4000-8000-0000000000b2', 'autobo', 'fiscal', 'active');
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
    db.batch_execute("SET ROLE identity_app").expect("papel");
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
