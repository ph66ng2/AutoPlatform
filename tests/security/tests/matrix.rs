//! Matriz de ameaças da fundação. Evidência sem segredo.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ap_contracts::{load_schema, validate_instance};
use ap_identity::{
    resolve, AccessError, JwtSecret, MembershipStatus, MemoryDirectory, PartyStatus, Permission,
    Product, Role,
};
use ap_observability::{capture_trace, observe_synthetic, redact, SyntheticInput};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::json;
use uuid::Uuid;

const SECRET: &[u8] = b"synthetic-jwt-hs256-key";

#[test]
fn tenant_a_never_receives_tenant_b_or_implied_fiscal() {
    let (directory, company_a, company_b, account_a, account_b) = directory();
    let secret = JwtSecret::new(SECRET.to_vec());
    let minted = token(account_a, company_b);
    let autoos = resolve(&secret, &directory, &minted, company_a, Product::AutoOs).expect("a");
    assert!(!autoos.allows(Permission::Fiscal));
    let cross = resolve(&secret, &directory, &minted, company_b, Product::AutoBo);
    let missing = resolve(
        &secret,
        &directory,
        &minted,
        Uuid::from_u128(0xdead),
        Product::AutoBo,
    );
    assert_eq!(cross, Err(AccessError::Denied));
    assert_eq!(missing, Err(AccessError::Denied));
    assert_eq!(format!("{cross:?}"), format!("{missing:?}"));
    let other = resolve(
        &secret,
        &directory,
        &token(account_b, company_a),
        company_b,
        Product::AutoBo,
    )
    .expect("b");
    assert!(other.allows(Permission::Fiscal));
}

#[test]
fn logs_and_contracts_drop_payload_and_unknown_version() {
    let secret = ["doc", "-secret-marker"].concat();
    let xml = ["<NFe>", secret.as_str(), "</NFe>"].concat();
    assert!(redact(&xml).contains("[REDACTED]"));
    assert!(!redact(&xml).contains(&secret));
    let trace = capture_trace(|| {
        let _ = observe_synthetic(SyntheticInput {
            raw_tenant: "tenant-raw-synthetic",
            unsafe_detail: &xml,
            queue_depth: 0,
            retries: 0,
            indeterminate: 0,
            provider_failures: 0,
        });
    });
    assert!(!trace.contains(&secret));
    assert!(!trace.contains("tenant-raw-synthetic"));

    let schema = load_schema("fiscal-document").expect("schema");
    let mut valid: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(contracts_dir().join("fixtures/valid/fiscal-document.json"))
            .expect("fx"),
    )
    .expect("json");
    assert!(validate_instance(&schema, &valid).is_ok());
    valid["schema_version"] = json!("9");
    assert!(validate_instance(&schema, &valid).is_err());
    valid["schema_version"] = json!("1");
    valid["payload"] = json!(xml);
    assert!(validate_instance(&schema, &valid).is_err());
}

#[test]
fn clients_do_not_carry_service_role_or_provider_tokens() {
    let root = repo_root();
    let clients = [
        root.join("examples"),
        root.join("openapi"),
        root.join("crates/portal"),
    ];
    let forbidden = [
        "service_role",
        "service-role",
        "SUPABASE_SERVICE_ROLE_KEY",
        "BEGIN PRIVATE KEY",
        "ghp_",
        "xoxb-",
        "AKIA",
    ];
    for dir in clients {
        for path in walk_files(&dir) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let lower = text.to_ascii_lowercase();
            for needle in forbidden {
                assert!(
                    !lower.contains(&needle.to_ascii_lowercase()),
                    "cliente com material de servidor: {}",
                    path.strip_prefix(&root).unwrap_or(&path).display()
                );
            }
        }
    }
}

#[test]
fn report_stays_no_go_until_human_and_has_no_secret() {
    let report = fs::read_to_string(repo_root().join("docs/reports/fundacao-sec-gate.md"))
        .expect("relatório");
    let lower = report.to_ascii_lowercase();
    assert!(lower.contains("veredito"));
    assert!(lower.contains("pendente"));
    assert!(!lower.contains("\ngo\n"));
    assert!(!lower.contains("veredito: go"));
    assert!(!report.contains("service_role"));
    assert!(!report.contains("super-secret-token"));
    assert!(!report.contains("AKIA"));
    assert!(report.contains("Owner: plataforma"));
    assert!(report.contains("## Riscos residuais"));
}

#[test]
fn dockerfile_and_env_example_do_not_embed_runtime_secrets() {
    let root = repo_root();
    let docker = fs::read_to_string(root.join("Dockerfile")).expect("Dockerfile");
    assert!(!docker.to_ascii_uppercase().contains("DATABASE_URL"));
    assert!(!docker.contains("SERVICE_ROLE"));
    assert!(docker.contains("USER nobody"));
    let example = fs::read_to_string(root.join(".env.example")).expect("env");
    for line in example.lines() {
        if line.starts_with("DATABASE_URL=") || line.starts_with("SUPABASE_SERVICE_ROLE_KEY=") {
            assert!(
                line.ends_with('='),
                "exemplo local não pode versionar segredo"
            );
        }
    }
}

fn directory() -> (MemoryDirectory, Uuid, Uuid, Uuid, Uuid) {
    let company_a = Uuid::from_u128(0xa1);
    let company_b = Uuid::from_u128(0xb1);
    let account_a = Uuid::from_u128(0xa2);
    let account_b = Uuid::from_u128(0xb2);
    let mut directory = MemoryDirectory::default();
    directory.insert_company(company_a, PartyStatus::Active);
    directory.insert_company(company_b, PartyStatus::Active);
    directory.insert_account(account_a, PartyStatus::Active);
    directory.insert_account(account_b, PartyStatus::Active);
    directory
        .insert_membership(
            Uuid::from_u128(0xa3),
            company_a,
            account_a,
            Product::AutoOs,
            Role::Admin,
            MembershipStatus::Active,
        )
        .expect("autoos");
    directory
        .insert_membership(
            Uuid::from_u128(0xb3),
            company_b,
            account_b,
            Product::AutoBo,
            Role::Fiscal,
            MembershipStatus::Active,
        )
        .expect("fiscal");
    (directory, company_a, company_b, account_a, account_b)
}

fn token(account: Uuid, company_claim: Uuid) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("relógio")
        .as_secs()
        + 600;
    let claims = json!({
        "sub": account.to_string(),
        "aud": "authenticated",
        "exp": exp,
        "role": "authenticated",
        "user_metadata": {"company_id": company_claim.to_string(), "role": "fiscal"},
        "company_id": company_claim.to_string(),
    });
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET),
    )
    .expect("jwt")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn contracts_dir() -> PathBuf {
    repo_root().join("contracts/v1")
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
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
