//! Ingestão imutável do FatoComercial v2 publicado pelo AutoOS.
use ap_contracts::{load_fato_comercial_v2, validate_fato_comercial_v2};
use ap_identity::{Permission, Session};
use postgres::{Client, Transaction};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

ap_kernel::declare_module!("integration");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestError {
    Invalid,
    Denied,
    NotFound,
    Conflict,
    Database,
}
impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "fato comercial inválido",
            Self::Denied => "acesso negado",
            Self::NotFound => "fato comercial não encontrado",
            Self::Conflict => "chave já usada com conteúdo diferente",
            Self::Database => "falha na persistência",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receipt {
    pub fact_id: Uuid,
    pub created: bool,
}

fn uuid(fact: &Value, key: &str) -> Result<Uuid, IngestError> {
    fact.get(key)
        .and_then(Value::as_str)
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(IngestError::Invalid)
}
fn field<'a>(fact: &'a Value, key: &str) -> Result<&'a str, IngestError> {
    fact.get(key)
        .and_then(Value::as_str)
        .ok_or(IngestError::Invalid)
}
fn authorized(tx: &mut Transaction<'_>, company: Uuid, account: Uuid) -> Result<(), IngestError> {
    tx.execute("SET LOCAL ROLE integration_app", &[])
        .map_err(|_| IngestError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id',$1,true),set_config('app.account_id',$2,true)",
        &[&company.to_string(), &account.to_string()],
    )
    .map_err(|_| IngestError::Database)?;
    let row = tx
        .query_one(
            "SELECT identity.has_autoos_permission($1,$2)",
            &[&company, &account],
        )
        .map_err(|_| IngestError::Database)?;
    if !row.get::<_, bool>(0) {
        return Err(IngestError::Denied);
    }
    Ok(())
}

fn authorized_fiscal(tx: &mut Transaction<'_>, session: Session) -> Result<(), IngestError> {
    if !session.allows(Permission::Fiscal) {
        return Err(IngestError::Denied);
    }
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| IngestError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id',$1,true),set_config('app.account_id',$2,true)",
        &[
            &session.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| IngestError::Database)?;
    let row = tx
        .query_one(
            "SELECT identity.has_fiscal_permission($1,$2)",
            &[&session.company_id, &session.account_id],
        )
        .map_err(|_| IngestError::Database)?;
    if !row.get::<_, bool>(0) {
        return Err(IngestError::Denied);
    }
    Ok(())
}

/// Leitura pelo núcleo fiscal, com membership FISCAL ativa e RLS do tenant.
pub fn find_for_fiscal(
    client: &mut Client,
    session: Session,
    fact_id: Uuid,
) -> Result<Value, IngestError> {
    let mut tx = client.transaction().map_err(|_| IngestError::Database)?;
    authorized_fiscal(&mut tx, session)?;
    let row = tx
        .query_opt(
            "SELECT payload::text FROM fiscal.fact_snapshots WHERE company_id=$1 AND fact_id=$2",
            &[&session.company_id, &fact_id],
        )
        .map_err(|_| IngestError::Database)?
        .ok_or(IngestError::NotFound)?;
    let fact = serde_json::from_str(row.get::<_, &str>(0)).map_err(|_| IngestError::Database)?;
    tx.commit().map_err(|_| IngestError::Database)?;
    Ok(fact)
}

pub fn find_for_fiscal_by_correlation(
    client: &mut Client,
    session: Session,
    correlation_id: &str,
) -> Result<Vec<Value>, IngestError> {
    let mut tx = client.transaction().map_err(|_| IngestError::Database)?;
    authorized_fiscal(&mut tx, session)?;
    let rows = tx.query(
        "SELECT payload::text FROM fiscal.fact_snapshots WHERE company_id=$1 AND correlation_id=$2 ORDER BY created_at,fact_id",
        &[&session.company_id, &correlation_id],
    ).map_err(|_| IngestError::Database)?;
    let facts = rows
        .iter()
        .map(|row| serde_json::from_str::<Value>(row.get::<_, &str>(0)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IngestError::Database)?;
    tx.commit().map_err(|_| IngestError::Database)?;
    Ok(facts)
}

/// Repetir uma entrega devolve o mesmo fact_id; qualquer chave divergente causa conflito.
pub fn ingest(
    client: &mut Client,
    company: Uuid,
    account: Uuid,
    fact: &Value,
) -> Result<Receipt, IngestError> {
    let schema = load_fato_comercial_v2().map_err(|_| IngestError::Database)?;
    validate_fato_comercial_v2(&schema, fact).map_err(|_| IngestError::Invalid)?;
    if uuid(fact, "company_id")? != company {
        return Err(IngestError::Denied);
    }
    let fact_id = uuid(fact, "fact_id")?;
    let event_id = uuid(fact, "event_id")?;
    let os_id = uuid(fact, "os_id")?;
    let os_version = fact
        .get("os_version")
        .and_then(Value::as_i64)
        .ok_or(IngestError::Invalid)?;
    let idempotency_key = field(fact, "idempotency_key")?;
    let correlation_id = field(fact, "correlation_id")?;
    let bytes = serde_json::to_vec(fact).map_err(|_| IngestError::Invalid)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let payload = String::from_utf8(bytes).map_err(|_| IngestError::Invalid)?;
    let mut tx = client.transaction().map_err(|_| IngestError::Database)?;
    authorized(&mut tx, company, account)?;
    let inserted = tx.query_opt(
        "INSERT INTO fiscal.fact_snapshots(company_id,fact_id,os_id,os_version,origin,payload_hash,payload,event_id,idempotency_key,correlation_id) VALUES($1,$2,$3,$4,'autoos',$5,($6::text)::jsonb,$7,$8,$9) ON CONFLICT DO NOTHING RETURNING fact_id",
        &[&company, &fact_id, &os_id, &os_version, &hash, &payload, &event_id, &idempotency_key, &correlation_id],
    ).map_err(|_| IngestError::Database)?;
    let rows = tx.query(
        "SELECT fact_id,os_id,os_version,event_id,idempotency_key,correlation_id,payload_hash FROM fiscal.fact_snapshots WHERE company_id=$1 AND (fact_id=$2 OR event_id=$3 OR idempotency_key=$4 OR (os_id=$5 AND os_version=$6))",
        &[&company, &fact_id, &event_id, &idempotency_key, &os_id, &os_version],
    ).map_err(|_| IngestError::Database)?;
    if rows.len() != 1 {
        return Err(IngestError::Conflict);
    }
    let row = &rows[0];
    if row.get::<_, Uuid>(0) != fact_id
        || row.get::<_, Uuid>(1) != os_id
        || row.get::<_, i64>(2) != os_version
        || row.get::<_, Option<Uuid>>(3) != Some(event_id)
        || row.get::<_, Option<&str>>(4) != Some(idempotency_key)
        || row.get::<_, Option<&str>>(5) != Some(correlation_id)
        || row.get::<_, &str>(6) != hash
    {
        return Err(IngestError::Conflict);
    }
    tx.commit().map_err(|_| IngestError::Database)?;
    Ok(Receipt {
        fact_id,
        created: inserted.is_some(),
    })
}

pub fn find(
    client: &mut Client,
    company: Uuid,
    account: Uuid,
    fact_id: Uuid,
) -> Result<Value, IngestError> {
    let mut tx = client.transaction().map_err(|_| IngestError::Database)?;
    authorized(&mut tx, company, account)?;
    let row = tx
        .query_opt(
            "SELECT payload::text FROM fiscal.fact_snapshots WHERE company_id=$1 AND fact_id=$2",
            &[&company, &fact_id],
        )
        .map_err(|_| IngestError::Database)?
        .ok_or(IngestError::NotFound)?;
    let payload: String = row.get(0);
    let fact = serde_json::from_str(&payload).map_err(|_| IngestError::Database)?;
    tx.commit().map_err(|_| IngestError::Database)?;
    Ok(fact)
}

pub fn find_by_correlation(
    client: &mut Client,
    company: Uuid,
    account: Uuid,
    correlation_id: &str,
) -> Result<Vec<Value>, IngestError> {
    let mut tx = client.transaction().map_err(|_| IngestError::Database)?;
    authorized(&mut tx, company, account)?;
    let rows = tx.query("SELECT payload::text FROM fiscal.fact_snapshots WHERE company_id=$1 AND correlation_id=$2 ORDER BY created_at,fact_id",
        &[&company, &correlation_id]).map_err(|_| IngestError::Database)?;
    let facts = rows
        .iter()
        .map(|row| serde_json::from_str::<Value>(row.get::<_, &str>(0)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IngestError::Database)?;
    tx.commit().map_err(|_| IngestError::Database)?;
    Ok(facts)
}
