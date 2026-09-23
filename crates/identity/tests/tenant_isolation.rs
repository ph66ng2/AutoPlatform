use std::{
    env, fs,
    path::{Path, PathBuf},
};

use postgres::{Client, NoTls};
use uuid::Uuid;

#[test]
fn rls_blocks_cross_tenant_reads_and_writes() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let _cleanup = ClusterGuard;
    let mut admin = connect("postgres");
    reset_cluster(&mut admin);
    admin
        .batch_execute("CREATE DATABASE identity_rls")
        .expect("cria banco");
    drop(admin);

    let mut db = connect("identity_rls");
    for file in ["0001_baseline.sql", "0002_identity.sql"] {
        let sql = fs::read_to_string(migration(file)).unwrap_or_else(|err| panic!("{file}: {err}"));
        db.batch_execute(&sql)
            .unwrap_or_else(|err| panic!("{file}: {err}"));
    }
    seed(&mut db);
    assert_policies_have_using_and_check(&mut db);

    let company_a = id("00000000-0000-4000-8000-0000000000a1");
    let company_b = id("00000000-0000-4000-8000-0000000000b1");
    let account_a = id("00000000-0000-4000-8000-0000000000a2");
    let touch_a = id("00000000-0000-4000-8000-0000000000a5");

    assume(&mut db, company_a, account_a);
    let accounts = db.query("SELECT id FROM identity.accounts", &[]);
    assert_eq!(
        accounts
            .expect_err("accounts")
            .code()
            .map(|code| code.code()),
        Some("42501")
    );
    db.execute(
        "INSERT INTO identity.session_touches (company_id, id, account_id) VALUES ($1, $2, $3)",
        &[&company_a, &touch_a, &account_a],
    )
    .expect("toque da própria company");
    let visible: i64 = db
        .query_one("SELECT count(*) FROM identity.session_touches", &[])
        .expect("contagem")
        .get(0);
    assert_eq!(visible, 1);
    let companies: i64 = db
        .query_one("SELECT count(*) FROM identity.companies", &[])
        .expect("companies")
        .get(0);
    assert_eq!(companies, 1);
    let foreign_companies: i64 = db
        .query_one(
            "SELECT count(*) FROM identity.companies WHERE id = $1",
            &[&company_b],
        )
        .expect("company b")
        .get(0);
    assert_eq!(foreign_companies, 0);

    let cross_insert = db.execute(
        "INSERT INTO identity.session_touches (company_id, id, account_id) VALUES ($1, $2, $3)",
        &[
            &company_b,
            &id("00000000-0000-4000-8000-0000000000b5"),
            &account_a,
        ],
    );
    assert_eq!(
        cross_insert
            .expect_err("escrita cruzada")
            .code()
            .map(|code| code.code()),
        Some("42501")
    );
    let moved = db.execute(
        "UPDATE identity.session_touches SET company_id = $1 WHERE id = $2",
        &[&company_b, &touch_a],
    );
    assert_eq!(
        moved
            .expect_err("mudança de company")
            .code()
            .map(|code| code.code()),
        Some("42501")
    );

    db.batch_execute("RESET ROLE").expect("reset");
    db.execute(
        "SELECT identity.set_membership_status($1, $2, 'suspended')",
        &[&company_a, &id("00000000-0000-4000-8000-0000000000a3")],
    )
    .expect("suspende");
    db.execute(
        "SELECT identity.set_membership_status($1, $2, 'revoked')",
        &[&company_a, &id("00000000-0000-4000-8000-0000000000a4")],
    )
    .expect("revoga");
    let audits: i64 = db
        .query_one(
            "SELECT count(*) FROM identity.audit_events WHERE company_id = $1",
            &[&company_a],
        )
        .expect("auditoria")
        .get(0);
    assert_eq!(audits, 2);

    assume(&mut db, company_a, account_a);
    let after_suspend: i64 = db
        .query_one("SELECT count(*) FROM identity.session_touches", &[])
        .expect("depois da suspensão")
        .get(0);
    assert_eq!(after_suspend, 0);
    let suspended_insert = db.execute(
        "INSERT INTO identity.session_touches (company_id, id, account_id) VALUES ($1, $2, $3)",
        &[
            &company_a,
            &id("00000000-0000-4000-8000-0000000000a6"),
            &account_a,
        ],
    );
    assert!(suspended_insert.is_err());

    assume(
        &mut db,
        company_b,
        id("00000000-0000-4000-8000-0000000000b2"),
    );
    let leaked_audit: i64 = db
        .query_one("SELECT count(*) FROM identity.audit_events", &[])
        .expect("auditoria alheia")
        .get(0);
    assert_eq!(leaked_audit, 0);
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
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'identity_rls' AND pid <> pg_backend_pid()",
        )
        .expect("encerra sessões");
    admin
        .batch_execute("DROP DATABASE IF EXISTS identity_rls")
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
            ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000a4', '00000000-0000-4000-8000-0000000000a2', 'autobo', 'admin', 'active'),
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

fn assert_policies_have_using_and_check(db: &mut Client) {
    let rows = db
        .query(
            "
            SELECT c.relname, pg_get_expr(p.polqual, p.polrelid), pg_get_expr(p.polwithcheck, p.polrelid)
            FROM pg_policy p
            JOIN pg_class c ON c.oid = p.polrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'identity'
            ",
            &[],
        )
        .expect("policies");
    assert!(rows.len() >= 4);
    for row in rows {
        let table: String = row.get(0);
        let using: Option<String> = row.get(1);
        let check: Option<String> = row.get(2);
        let using = using.unwrap_or_else(|| panic!("{table} sem USING"));
        let check = check.unwrap_or_else(|| panic!("{table} sem WITH CHECK"));
        assert!(using.contains("current_company_id"), "{table}");
        assert!(check.contains("current_company_id"), "{table}");
    }
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
