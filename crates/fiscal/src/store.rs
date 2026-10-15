use ap_identity::{Permission, Session};
use postgres::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::model::{
    DecisionResolver, DocumentState, FiscalError, HumanConfirmation, PreparedDocument,
};
use crate::plan::prepare_plan;

fn hash<T: serde::Serialize>(value: &T) -> Result<String, FiscalError> {
    let bytes = serde_json::to_vec(value).map_err(|_| FiscalError::InvalidFact)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Prepara documentos, sem chamar um emissor. A decisão vem de um resolvedor
/// confiável e versionado; uma entrada da API não pode fornecer a própria regra.
pub fn prepare(
    client: &mut Client,
    session: Session,
    profile_id: Uuid,
    fact: &Value,
    scenario: &str,
    at_epoch: i64,
    resolver: &impl DecisionResolver,
) -> Result<Vec<PreparedDocument>, FiscalError> {
    let plan = prepare_plan(session, fact, scenario, at_epoch, resolver)?;
    let fact_hash = hash(fact)?;
    let fact_json = fact.to_string();
    let mut tx = client.transaction().map_err(|_| FiscalError::Database)?;
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| FiscalError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id', $1, true), set_config('app.account_id', $2, true)",
        &[
            &plan.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| FiscalError::Database)?;
    let profile = tx.query_opt(
        "SELECT 1 FROM fiscal.issuer_profiles WHERE company_id=$1 AND id=$2 AND establishment_id=$3",
        &[&plan.company_id, &profile_id, &plan.establishment_id],
    ).map_err(|_| FiscalError::Database)?;
    if profile.is_none() {
        return Err(FiscalError::DecisionNotApplicable);
    }
    tx.execute(
        "INSERT INTO fiscal.fact_snapshots(company_id,fact_id,os_id,os_version,origin,payload_hash,payload) VALUES($1,$2,$3,$4,'autoos',$5,($6::text)::jsonb) ON CONFLICT DO NOTHING",
        &[&plan.company_id, &plan.fact_id, &plan.os_id, &plan.os_version, &fact_hash, &fact_json],
    ).map_err(|_| FiscalError::Database)?;
    let snapshot = tx.query_opt(
        "SELECT fact_id,payload_hash FROM fiscal.fact_snapshots WHERE company_id=$1 AND os_id=$2 AND os_version=$3",
        &[&plan.company_id, &plan.os_id, &plan.os_version],
    ).map_err(|_| FiscalError::Database)?.ok_or(FiscalError::Conflict)?;
    if snapshot.get::<_, Uuid>(0) != plan.fact_id || snapshot.get::<_, String>(1) != fact_hash {
        return Err(FiscalError::Conflict);
    }
    let mut result = Vec::with_capacity(plan.decision.documents.len());
    for document in &plan.decision.documents {
        let plan_hash = hash(&(plan.decision.version, &plan.decision.scenario, document))?;
        let id = Uuid::new_v4();
        let inserted = tx.query_opt(
            "INSERT INTO fiscal.documents(company_id,id,fact_id,profile_id,plan_key,plan_hash,decision_version,scenario,kind,fiscal_code,amount_cents,tax_base_cents) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING RETURNING id",
            &[&plan.company_id, &id, &plan.fact_id, &profile_id, &document.key, &plan_hash, &plan.decision.version, &scenario, &document.kind.as_str(), &document.fiscal_code, &document.amount_cents, &document.tax_base_cents],
        ).map_err(|_| FiscalError::Database)?;
        let row = tx.query_one(
            "SELECT id,profile_id,plan_hash,kind,fiscal_code,amount_cents,tax_base_cents,state FROM fiscal.documents WHERE company_id=$1 AND fact_id=$2 AND plan_key=$3",
            &[&plan.company_id, &plan.fact_id, &document.key],
        ).map_err(|_| FiscalError::Database)?;
        if row.get::<_, Uuid>(1) != profile_id
            || row.get::<_, String>(2) != plan_hash
            || row.get::<_, String>(3) != document.kind.as_str()
            || row.get::<_, String>(4) != document.fiscal_code
            || row.get::<_, i64>(5) != document.amount_cents
            || row.get::<_, i64>(6) != document.tax_base_cents
        {
            return Err(FiscalError::Conflict);
        }
        let persisted_id: Uuid = row.get(0);
        if inserted.is_some() {
            for item_id in &document.item_ids {
                tx.execute("INSERT INTO fiscal.document_items(company_id,document_id,item_id) VALUES($1,$2,$3)", &[&plan.company_id, &persisted_id, item_id])
                    .map_err(|_| FiscalError::Database)?;
            }
            tx.execute("INSERT INTO fiscal.document_events(company_id,id,document_id,actor_id,kind) VALUES($1,$2,$3,$4,'prepared')", &[&plan.company_id, &Uuid::new_v4(), &persisted_id, &session.account_id])
                .map_err(|_| FiscalError::Database)?;
        }
        result.push(PreparedDocument {
            id: persisted_id,
            company_id: plan.company_id,
            fact_id: plan.fact_id,
            plan_key: document.key.clone(),
            kind: document.kind,
            state: DocumentState::from_db(row.get::<_, &str>(7)).ok_or(FiscalError::Database)?,
        });
    }
    tx.commit().map_err(|_| FiscalError::Database)?;
    Ok(result)
}

/// Persiste uma transição decorrente de confirmação humana ou retorno do emissor.
/// Um timeout indeterminado só admite consulta antes de qualquer nova tentativa.
pub fn transition(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    expected: DocumentState,
    next: DocumentState,
    confirmation: Option<HumanConfirmation>,
    xml: Option<&[u8]>,
) -> Result<(), FiscalError> {
    if !session.allows(Permission::Fiscal) {
        return Err(FiscalError::Denied);
    }
    if !expected.permits(next) {
        return Err(FiscalError::InvalidTransition);
    }
    let needs_human = matches!(next, DocumentState::Sending | DocumentState::CancelPending);
    if needs_human && confirmation.map(|c| c.actor_id) != Some(session.account_id) {
        return Err(FiscalError::Denied);
    }
    if next == DocumentState::Authorized && xml.is_none_or(<[u8]>::is_empty) {
        return Err(FiscalError::XmlRequired);
    }
    let mut tx = client.transaction().map_err(|_| FiscalError::Database)?;
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| FiscalError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id', $1, true), set_config('app.account_id', $2, true)",
        &[
            &session.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| FiscalError::Database)?;
    let row = tx
        .query_opt(
            "SELECT state FROM fiscal.documents WHERE company_id=$1 AND id=$2 FOR UPDATE",
            &[&session.company_id, &document_id],
        )
        .map_err(|_| FiscalError::Database)?
        .ok_or(FiscalError::NotFound)?;
    if row.get::<_, &str>(0) != expected.as_str() {
        return Err(FiscalError::Conflict);
    }
    if let Some(bytes) = xml {
        if next == DocumentState::Authorized {
            tx.execute("INSERT INTO fiscal.document_artifacts(company_id,document_id,kind,content) VALUES($1,$2,'xml',$3) ON CONFLICT DO NOTHING", &[&session.company_id, &document_id, &bytes])
                .map_err(|_| FiscalError::Database)?;
        }
    }
    tx.execute(
        "UPDATE fiscal.documents SET state=$3 WHERE company_id=$1 AND id=$2",
        &[&session.company_id, &document_id, &next.as_str()],
    )
    .map_err(|_| FiscalError::Database)?;
    let event = match next {
        DocumentState::Sending => "issue_confirmed",
        DocumentState::Authorized => "authorized",
        DocumentState::Rejected => "rejected",
        DocumentState::Indeterminate => "indeterminate",
        DocumentState::CancelPending => "cancel_confirmed",
        DocumentState::Cancelled => "cancelled",
        DocumentState::Prepared => return Err(FiscalError::InvalidTransition),
    };
    tx.execute("INSERT INTO fiscal.document_events(company_id,id,document_id,actor_id,kind) VALUES($1,$2,$3,$4,$5)", &[&session.company_id, &Uuid::new_v4(), &document_id, &session.account_id, &event])
        .map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)?;
    Ok(())
}
