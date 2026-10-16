use std::{
    env, fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
};

use ap_fiscal::{
    cancel, consult, issue, prepare, prepare_plan, transition, DecisionResolver, DocumentKind,
    DocumentPlan, DocumentState, FiscalDecision, FiscalError, FiscalProvider, HumanConfirmation,
    ProviderOutcome, ProviderRequest,
};
use ap_identity::{Product, Role, Session};
use postgres::{Client, NoTls};
use serde_json::Value;
use uuid::Uuid;

fn id(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("uuid")
}
fn company() -> Uuid {
    id("00000000-0000-4000-8000-0000000000a1")
}
fn account() -> Uuid {
    id("00000000-0000-4000-8000-0000000000a2")
}
fn establishment() -> Uuid {
    id("00000000-0000-4000-8000-0000000000b1")
}
fn profile() -> Uuid {
    id("00000000-0000-4000-8000-0000000000c1")
}
fn session() -> Session {
    Session {
        company_id: company(),
        account_id: account(),
        product: Product::AutoBo,
        role: Role::Fiscal,
    }
}
fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v2/examples/valid")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("json")
}
fn decision(fact: &Value, kind: DocumentKind, code: &str) -> FiscalDecision {
    FiscalDecision {
        company_id: company(),
        establishment_id: establishment(),
        scenario: "os-maintenance".into(),
        version: 1,
        approved_by: account(),
        approved_at_epoch: 1,
        effective_from_epoch: 1,
        effective_until_epoch: None,
        documents: vec![DocumentPlan {
            key: "document-1".into(),
            kind,
            amount_cents: fact["totals"]["total_cents"].as_i64().expect("total"),
            tax_base_cents: fact["totals"]["total_cents"].as_i64().expect("total"),
            fiscal_code: code.into(),
            item_ids: fact["items"]
                .as_array()
                .expect("items")
                .iter()
                .map(|item| item["item_id"].as_str().expect("item id").to_owned())
                .collect(),
        }],
    }
}
struct Resolver(Option<FiscalDecision>);
struct FakeProvider {
    calls: AtomicUsize,
}
impl FiscalProvider for FakeProvider {
    fn issue(&self, req: &ProviderRequest) -> ProviderOutcome {
        assert_eq!(req.fiscal_code, "14.02");
        assert_eq!(req.amount_cents, 320_000);
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderOutcome::Indeterminate
    }
    fn consult(&self, _req: &ProviderRequest) -> ProviderOutcome {
        ProviderOutcome::Authorized {
            xml: b"<xml>synthetic</xml>".to_vec(),
        }
    }
    fn cancel(&self, _req: &ProviderRequest) -> ProviderOutcome {
        ProviderOutcome::Cancelled
    }
}
struct FixedProvider {
    consultation: ProviderOutcome,
    cancellation: ProviderOutcome,
}
impl FiscalProvider for FixedProvider {
    fn issue(&self, _: &ProviderRequest) -> ProviderOutcome {
        ProviderOutcome::Indeterminate
    }
    fn cancel(&self, _: &ProviderRequest) -> ProviderOutcome {
        self.cancellation.clone()
    }
    fn consult(&self, _: &ProviderRequest) -> ProviderOutcome {
        self.consultation.clone()
    }
}
impl DecisionResolver for Resolver {
    fn approved_decision(
        &self,
        _company: Uuid,
        _establishment: Uuid,
        _scenario: &str,
        _at: i64,
    ) -> Result<Option<FiscalDecision>, FiscalError> {
        Ok(self.0.clone())
    }
}
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

#[test]
fn approved_decisions_are_required_and_bundle_values_are_explicit() {
    let fact = fixture("mista-preco-unico.json");
    assert!(matches!(
        prepare_plan(session(), &fact, "os-maintenance", 2, &Resolver(None)),
        Err(FiscalError::DecisionMissing)
    ));
    let approved = decision(&fact, DocumentKind::Nfse, "14.02");
    prepare_plan(
        session(),
        &fact,
        "os-maintenance",
        2,
        &Resolver(Some(approved.clone())),
    )
    .expect("bundle explicit");
    let mut invalid = approved;
    invalid.documents[0].amount_cents -= 1;
    assert!(matches!(
        prepare_plan(
            session(),
            &fact,
            "os-maintenance",
            2,
            &Resolver(Some(invalid))
        ),
        Err(FiscalError::ValuesMissing)
    ));
    for name in ["servico.json", "produto.json", "mista.json"] {
        let fact = fixture(name);
        let kind = if name == "produto.json" {
            DocumentKind::Nfe
        } else {
            DocumentKind::Nfse
        };
        let mut approved = decision(&fact, kind, "explicit-code");
        if name == "mista.json" {
            approved.documents = vec![
                DocumentPlan {
                    key: "product".into(),
                    kind: DocumentKind::Nfe,
                    amount_cents: 19_000,
                    tax_base_cents: 19_000,
                    fiscal_code: "synthetic-product".into(),
                    item_ids: vec!["produto-1".into()],
                },
                DocumentPlan {
                    key: "service".into(),
                    kind: DocumentKind::Nfse,
                    amount_cents: 15_000,
                    tax_base_cents: 15_000,
                    fiscal_code: "synthetic-service".into(),
                    item_ids: vec!["servico-1".into()],
                },
            ];
        }
        prepare_plan(
            session(),
            &fact,
            "os-maintenance",
            2,
            &Resolver(Some(approved)),
        )
        .expect("itemized explicit");
    }
}

#[test]
fn tenant_isolation_concurrent_prepare_and_state_machine() {
    if env::var("AUTO_PLATFORM_CI_DATABASE").ok().as_deref() != Some("1") {
        return;
    }
    let mut admin = db("postgres");
    admin.batch_execute("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='fiscal_core' AND pid<>pg_backend_pid()").expect("terminate");
    admin
        .batch_execute("DROP DATABASE IF EXISTS fiscal_core")
        .expect("drop database");
    admin
        .batch_execute("CREATE DATABASE fiscal_core")
        .expect("create database");
    drop(admin);
    let mut client = db("fiscal_core");
    for file in [
        "0001_baseline.sql",
        "0002_identity.sql",
        "0003_inventory.sql",
        "0004_fiscal.sql",
    ] {
        let sql = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .join(file),
        )
        .expect("migration");
        client.batch_execute(&sql).expect("apply migration");
    }
    client.batch_execute("INSERT INTO identity.companies(id,label,status) VALUES ('00000000-0000-4000-8000-0000000000a1','a','active'),('00000000-0000-4000-8000-0000000000d1','b','active');
      INSERT INTO identity.accounts(id,status) VALUES ('00000000-0000-4000-8000-0000000000a2','active'),('00000000-0000-4000-8000-0000000000d2','active');
      INSERT INTO identity.memberships(company_id,id,account_id,product,role,status) VALUES
      ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000a3','00000000-0000-4000-8000-0000000000a2','autobo','fiscal','active'),
      ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d3','00000000-0000-4000-8000-0000000000d2','autobo','fiscal','active');
      INSERT INTO fiscal.issuer_profiles(company_id,id,establishment_id,label) VALUES
      ('00000000-0000-4000-8000-0000000000a1','00000000-0000-4000-8000-0000000000c1','00000000-0000-4000-8000-0000000000b1','a'),
      ('00000000-0000-4000-8000-0000000000d1','00000000-0000-4000-8000-0000000000d4','00000000-0000-4000-8000-0000000000d5','b');").expect("seed");
    // Um envio iniciado na versão anterior precisa continuar consultável após a migration aditiva.
    let old_fact = fixture("servico.json").to_string();
    let old_id = id("00000000-0000-4000-8000-0000000000c4");
    client.execute("INSERT INTO fiscal.fact_snapshots(company_id,fact_id,os_id,os_version,origin,payload_hash,payload) VALUES($1,$2,$3,1,'autoos',$4,($5::text)::jsonb)",
        &[&company(), &id("00000000-0000-4000-8000-0000000000c2"), &id("00000000-0000-4000-8000-0000000000c3"), &"a".repeat(64), &old_fact]).expect("old fact");
    client.execute("INSERT INTO fiscal.documents(company_id,id,fact_id,profile_id,plan_key,plan_hash,decision_version,scenario,kind,fiscal_code,amount_cents,tax_base_cents) VALUES($1,$2,$3,$4,'old-plan',$5,1,'os-maintenance','nfse','14.02',15000,15000)",
        &[&company(), &old_id, &id("00000000-0000-4000-8000-0000000000c2"), &profile(), &"b".repeat(64)]).expect("old document");
    client
        .execute(
            "UPDATE fiscal.documents SET state='sending' WHERE company_id=$1 AND id=$2",
            &[&company(), &old_id],
        )
        .expect("old in flight");
    client.execute("INSERT INTO fiscal.document_events(company_id,id,document_id,actor_id,kind) VALUES($1,$2,$3,$4,'issue_confirmed')",
        &[&company(), &Uuid::new_v4(), &old_id, &account()]).expect("old event");
    let sql = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations/0005_fiscal_operation_recovery.sql"),
    )
    .expect("migration 0005");
    client.batch_execute(&sql).expect("additive migration");
    let recovered = FixedProvider {
        consultation: ProviderOutcome::Authorized {
            xml: b"<xml>old</xml>".to_vec(),
        },
        cancellation: ProviderOutcome::Cancelled,
    };
    assert_eq!(
        consult(&mut client, session(), old_id, &recovered).expect("old in-flight recovery"),
        DocumentState::Authorized
    );
    let fact = fixture("mista-preco-unico.json");
    let resolver = Resolver(Some(decision(&fact, DocumentKind::Nfse, "14.02")));
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let barrier = Arc::clone(&barrier);
        let fact = fact.clone();
        let approved = resolver.0.clone();
        handles.push(thread::spawn(move || {
            let mut db = db("fiscal_core");
            barrier.wait();
            prepare(
                &mut db,
                session(),
                profile(),
                &fact,
                "os-maintenance",
                2,
                &Resolver(approved),
            )
        }));
    }
    let first = handles.remove(0).join().expect("thread").expect("prepare");
    let second = handles
        .remove(0)
        .join()
        .expect("thread")
        .expect("idempotent");
    assert_eq!(first[0].id, second[0].id);
    assert_eq!(first[0].state, DocumentState::Prepared);
    let b = Session {
        company_id: id("00000000-0000-4000-8000-0000000000d1"),
        account_id: id("00000000-0000-4000-8000-0000000000d2"),
        product: Product::AutoBo,
        role: Role::Fiscal,
    };
    assert_eq!(
        transition(
            &mut client,
            b,
            first[0].id,
            DocumentState::Prepared,
            DocumentState::Sending,
            Some(HumanConfirmation {
                actor_id: b.account_id
            }),
            None
        ),
        Err(FiscalError::NotFound)
    );
    assert_eq!(
        transition(
            &mut client,
            session(),
            first[0].id,
            DocumentState::Prepared,
            DocumentState::Sending,
            None,
            None
        ),
        Err(FiscalError::Denied)
    );
    let provider = FakeProvider {
        calls: AtomicUsize::new(0),
    };
    assert_eq!(
        issue(
            &mut client,
            session(),
            first[0].id,
            HumanConfirmation {
                actor_id: account()
            },
            &provider
        )
        .expect("issue"),
        DocumentState::Indeterminate
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        issue(
            &mut client,
            session(),
            first[0].id,
            HumanConfirmation {
                actor_id: account()
            },
            &provider
        ),
        Err(FiscalError::Conflict)
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        transition(
            &mut client,
            session(),
            first[0].id,
            DocumentState::Indeterminate,
            DocumentState::Sending,
            Some(HumanConfirmation {
                actor_id: account()
            }),
            None
        ),
        Err(FiscalError::InvalidTransition)
    );
    assert_eq!(
        transition(
            &mut client,
            session(),
            first[0].id,
            DocumentState::Indeterminate,
            DocumentState::Authorized,
            None,
            None
        ),
        Err(FiscalError::XmlRequired)
    );
    assert_eq!(
        consult(&mut client, session(), first[0].id, &provider).expect("consulted xml"),
        DocumentState::Authorized
    );
    assert_eq!(
        cancel(
            &mut client,
            session(),
            first[0].id,
            HumanConfirmation {
                actor_id: account()
            },
            &provider
        )
        .expect("cancelled"),
        DocumentState::Cancelled
    );
    let second_fact = fixture("servico.json");
    let second = prepare(
        &mut client,
        session(),
        profile(),
        &second_fact,
        "os-maintenance",
        2,
        &Resolver(Some(decision(&second_fact, DocumentKind::Nfse, "14.02"))),
    )
    .expect("second fact");
    let second_id = second[0].id;
    // Simula término do processo depois de persistir a intenção, antes de salvar a resposta.
    transition(
        &mut client,
        session(),
        second_id,
        DocumentState::Prepared,
        DocumentState::Sending,
        Some(HumanConfirmation {
            actor_id: account(),
        }),
        None,
    )
    .expect("in flight issue");
    let authorized = FixedProvider {
        consultation: ProviderOutcome::Authorized {
            xml: b"<xml>recovered</xml>".to_vec(),
        },
        cancellation: ProviderOutcome::Indeterminate,
    };
    assert_eq!(
        consult(&mut client, session(), second_id, &authorized).expect("recover issue"),
        DocumentState::Authorized
    );
    assert_eq!(
        cancel(
            &mut client,
            session(),
            second_id,
            HumanConfirmation {
                actor_id: account()
            },
            &authorized
        )
        .expect("cancel timeout"),
        DocumentState::Indeterminate
    );
    let rejected_cancel = FixedProvider {
        consultation: ProviderOutcome::Rejected,
        cancellation: ProviderOutcome::Cancelled,
    };
    assert_eq!(
        consult(&mut client, session(), second_id, &rejected_cancel)
            .expect("rejected cancellation"),
        DocumentState::Authorized
    );
    transition(
        &mut client,
        session(),
        second_id,
        DocumentState::Authorized,
        DocumentState::CancelPending,
        Some(HumanConfirmation {
            actor_id: account(),
        }),
        None,
    )
    .expect("in flight cancel");
    assert_eq!(
        consult(&mut client, session(), second_id, &rejected_cancel)
            .expect("recover pending cancel"),
        DocumentState::Authorized
    );
    assert_eq!(
        cancel(
            &mut client,
            session(),
            second_id,
            HumanConfirmation {
                actor_id: account()
            },
            &rejected_cancel
        )
        .expect("retry confirmed cancellation"),
        DocumentState::Cancelled
    );
    drop(client);
    let mut admin = db("postgres");
    admin
        .batch_execute("DROP DATABASE fiscal_core")
        .expect("cleanup");
}
