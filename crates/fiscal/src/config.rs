use std::sync::Mutex;

use ap_identity::{Permission, Session};
use postgres::{Client, Transaction};
use uuid::Uuid;

use crate::model::{DecisionResolver, DocumentPlan, FiscalDecision, FiscalError};

fn begin(client: &mut Client, session: Session) -> Result<Transaction<'_>, FiscalError> {
    if !session.allows(Permission::Fiscal) {
        return Err(FiscalError::Denied);
    }
    let mut tx = client.transaction().map_err(|_| FiscalError::Database)?;
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| FiscalError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id',$1,true), set_config('app.account_id',$2,true)",
        &[
            &session.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| FiscalError::Database)?;
    Ok(tx)
}

pub fn create_issuer_profile(
    client: &mut Client,
    session: Session,
    profile_id: Uuid,
    establishment_id: Uuid,
    label: &str,
) -> Result<(), FiscalError> {
    if profile_id.is_nil() || establishment_id.is_nil() || label.trim().is_empty() {
        return Err(FiscalError::InvalidFact);
    }
    let mut tx = begin(client, session)?;
    tx.execute(
        "INSERT INTO fiscal.issuer_profiles(company_id,id,establishment_id,label) VALUES($1,$2,$3,$4)",
        &[&session.company_id, &profile_id, &establishment_id, &label],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

/// Completa o perfil já criado para o estabelecimento. O estado draft bloqueia
/// prepare; este método não aprova uma decisão fiscal por si só.
pub fn complete_issuer_profile(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    municipality_ibge_code: &str,
    tax_regime: &str,
) -> Result<(), FiscalError> {
    if municipality_ibge_code.len() != 7
        || !municipality_ibge_code.bytes().all(|b| b.is_ascii_digit())
        || tax_regime.trim().is_empty()
    {
        return Err(FiscalError::DecisionNotApplicable);
    }
    let mut tx = begin(client, session)?;
    let changed = tx.execute(
        "UPDATE fiscal.issuer_profiles SET municipality_ibge_code=$3,tax_regime=$4,review_status='ready' WHERE company_id=$1 AND establishment_id=$2",
        &[&session.company_id, &establishment_id, &municipality_ibge_code, &tax_regime],
    ).map_err(|_| FiscalError::Database)?;
    if changed != 1 {
        return Err(FiscalError::NotFound);
    }
    tx.commit().map_err(|_| FiscalError::Database)
}

/// Metadados de item preservam a origem declarada da peça. Não escolhem nota.
pub fn register_item(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    item_key: &str,
    description: &str,
    item_kind: &str,
    piece_source: &str,
) -> Result<(), FiscalError> {
    if item_key.trim().is_empty()
        || description.trim().is_empty()
        || !["service", "product"].contains(&item_kind)
        || !["none", "customer", "workshop"].contains(&piece_source)
    {
        return Err(FiscalError::InvalidFact);
    }
    let mut tx = begin(client, session)?;
    tx.execute(
        "INSERT INTO fiscal.item_profiles(company_id,establishment_id,item_key,description,item_kind,piece_source) VALUES($1,$2,$3,$4,$5,$6)",
        &[&session.company_id, &establishment_id, &item_key, &description, &item_kind, &piece_source],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

pub fn approve_item(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    item_key: &str,
) -> Result<(), FiscalError> {
    let mut tx = begin(client, session)?;
    let changed = tx.execute(
        "UPDATE fiscal.item_profiles SET reviewed_by=$4,reviewed_at=clock_timestamp() WHERE company_id=$1 AND establishment_id=$2 AND item_key=$3 AND reviewed_at IS NULL",
        &[&session.company_id, &establishment_id, &item_key, &session.account_id],
    ).map_err(|_| FiscalError::Database)?;
    if changed != 1 {
        return Err(FiscalError::NotFound);
    }
    tx.commit().map_err(|_| FiscalError::Database)
}

/// Cenário explicita a combinação operacional; código fiscal só entra na
/// decisão revisada para uma OS concreta.
pub fn register_scenario(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    scenario: &str,
    operation_type: &str,
    piece_source: &str,
    pricing_mode: &str,
) -> Result<(), FiscalError> {
    if scenario.trim().is_empty()
        || !["service", "goods", "mixed"].contains(&operation_type)
        || !["none", "customer", "workshop"].contains(&piece_source)
        || !["itemized", "bundle"].contains(&pricing_mode)
    {
        return Err(FiscalError::InvalidFact);
    }
    let mut tx = begin(client, session)?;
    tx.execute(
        "INSERT INTO fiscal.operation_scenarios(company_id,establishment_id,scenario,operation_type,piece_source,pricing_mode) VALUES($1,$2,$3,$4,$5,$6)",
        &[&session.company_id, &establishment_id, &scenario, &operation_type, &piece_source, &pricing_mode],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

pub fn approve_scenario(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    scenario: &str,
    effective_from_epoch: i64,
    effective_until_epoch: Option<i64>,
    evidence_ref: &str,
) -> Result<(), FiscalError> {
    validate_approval(effective_from_epoch, effective_until_epoch, evidence_ref)?;
    let mut tx = begin(client, session)?;
    let changed = tx.execute(
        "UPDATE fiscal.operation_scenarios SET review_status='approved',approved_by=$4,approved_at=clock_timestamp(),effective_from=to_timestamp($5::bigint::double precision),effective_until=to_timestamp($6::bigint::double precision),evidence_ref=$7 WHERE company_id=$1 AND establishment_id=$2 AND scenario=$3 AND review_status='draft'",
        &[&session.company_id, &establishment_id, &scenario, &session.account_id, &effective_from_epoch, &effective_until_epoch, &evidence_ref],
    ).map_err(|_| FiscalError::Database)?;
    if changed != 1 {
        return Err(FiscalError::NotFound);
    }
    tx.commit().map_err(|_| FiscalError::Database)
}

pub fn save_decision_draft(
    client: &mut Client,
    session: Session,
    establishment_id: Uuid,
    scenario: &str,
    fact_id: Uuid,
    version: i64,
    documents: &[DocumentPlan],
) -> Result<(), FiscalError> {
    if version <= 0 || fact_id.is_nil() || documents.is_empty() {
        return Err(FiscalError::ValuesMissing);
    }
    let json = serde_json::to_string(documents).map_err(|_| FiscalError::ValuesMissing)?;
    let mut tx = begin(client, session)?;
    tx.execute(
        "INSERT INTO fiscal.decisions(company_id,establishment_id,scenario,fact_id,version,documents) VALUES($1,$2,$3,$4,$5,($6::text)::jsonb)",
        &[&session.company_id, &establishment_id, &scenario, &fact_id, &version, &json],
    ).map_err(|_| FiscalError::Database)?;
    tx.commit().map_err(|_| FiscalError::Database)
}

pub struct DecisionApproval<'a> {
    pub establishment_id: Uuid,
    pub scenario: &'a str,
    pub fact_id: Uuid,
    pub version: i64,
    pub effective_from_epoch: i64,
    pub effective_until_epoch: Option<i64>,
    pub evidence_ref: &'a str,
}

pub fn approve_decision(
    client: &mut Client,
    session: Session,
    approval: DecisionApproval<'_>,
) -> Result<(), FiscalError> {
    validate_approval(
        approval.effective_from_epoch,
        approval.effective_until_epoch,
        approval.evidence_ref,
    )?;
    let mut tx = begin(client, session)?;
    let changed = tx.execute(
        "UPDATE fiscal.decisions SET review_status='approved',approved_by=$6,approved_at=clock_timestamp(),effective_from=to_timestamp($7::bigint::double precision),effective_until=to_timestamp($8::bigint::double precision),evidence_ref=$9 WHERE company_id=$1 AND establishment_id=$2 AND scenario=$3 AND fact_id=$4 AND version=$5 AND review_status='draft'",
        &[&session.company_id, &approval.establishment_id, &approval.scenario, &approval.fact_id, &approval.version, &session.account_id, &approval.effective_from_epoch, &approval.effective_until_epoch, &approval.evidence_ref],
    ).map_err(|_| FiscalError::Database)?;
    if changed != 1 {
        return Err(FiscalError::NotFound);
    }
    tx.commit().map_err(|_| FiscalError::Database)
}

fn validate_approval(from: i64, until: Option<i64>, evidence_ref: &str) -> Result<(), FiscalError> {
    if from <= 0 || until.is_some_and(|end| end <= from) || evidence_ref.trim().is_empty() {
        Err(FiscalError::DecisionNotApplicable)
    } else {
        Ok(())
    }
}

/// Possui sua própria conexão: o resolvedor nunca aceita documentos fiscais
/// enviados pela requisição prepare e usa RLS com a sessão FISCAL.
pub struct PgDecisionResolver {
    client: Mutex<Client>,
    session: Session,
}

impl PgDecisionResolver {
    pub fn new(client: Client, session: Session) -> Self {
        Self {
            client: Mutex::new(client),
            session,
        }
    }
}

impl DecisionResolver for PgDecisionResolver {
    fn approved_decision(
        &self,
        company_id: Uuid,
        establishment_id: Uuid,
        scenario: &str,
        fact: &serde_json::Value,
        at_epoch: i64,
    ) -> Result<Option<FiscalDecision>, FiscalError> {
        if company_id != self.session.company_id {
            return Err(FiscalError::Denied);
        }
        let fact_id = fact["fact_id"]
            .as_str()
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .ok_or(FiscalError::InvalidFact)?;
        let pricing_mode = fact["pricing_mode"]
            .as_str()
            .ok_or(FiscalError::InvalidFact)?;
        let items = fact["items"].as_array().ok_or(FiscalError::InvalidFact)?;
        let has_service = items.iter().any(|item| item["kind"] == "service");
        let has_product = items.iter().any(|item| item["kind"] == "product");
        let operation_type = match (has_service, has_product) {
            (true, true) => "mixed",
            (true, false) => "service",
            (false, true) => "goods",
            (false, false) => return Err(FiscalError::InvalidFact),
        };
        let mut client = self.client.lock().map_err(|_| FiscalError::Database)?;
        let mut tx = begin(&mut client, self.session)?;
        let row = tx.query_opt(
            "SELECT d.version,d.approved_by,extract(epoch from d.approved_at)::bigint,extract(epoch from d.effective_from)::bigint,extract(epoch from d.effective_until)::bigint,d.documents::text,s.piece_source FROM fiscal.decisions d JOIN fiscal.operation_scenarios s ON (s.company_id=d.company_id AND s.establishment_id=d.establishment_id AND s.scenario=d.scenario) JOIN fiscal.issuer_profiles p ON (p.company_id=d.company_id AND p.establishment_id=d.establishment_id) WHERE d.company_id=$1 AND d.establishment_id=$2 AND d.scenario=$3 AND d.fact_id=$4 AND d.review_status='approved' AND s.review_status='approved' AND p.review_status='ready' AND s.pricing_mode=$6 AND s.operation_type=$7 AND d.approved_at <= to_timestamp($5::bigint::double precision) AND s.approved_at <= to_timestamp($5::bigint::double precision) AND d.effective_from <= to_timestamp($5::bigint::double precision) AND (d.effective_until IS NULL OR d.effective_until > to_timestamp($5::bigint::double precision)) AND s.effective_from <= to_timestamp($5::bigint::double precision) AND (s.effective_until IS NULL OR s.effective_until > to_timestamp($5::bigint::double precision)) ORDER BY d.version DESC LIMIT 1",
            &[&company_id, &establishment_id, &scenario, &fact_id, &at_epoch, &pricing_mode, &operation_type],
        ).map_err(|_| FiscalError::Database)?;
        if let Some(ref decision) = row {
            let expected_source: &str = decision.get(6);
            let mut customer_piece = false;
            let mut workshop_piece = false;
            for item in items {
                let item_id = item["item_id"].as_str().ok_or(FiscalError::InvalidFact)?;
                let item_kind = item["kind"].as_str().ok_or(FiscalError::InvalidFact)?;
                let profile = tx.query_opt(
                    "SELECT item_kind,piece_source FROM fiscal.item_profiles WHERE company_id=$1 AND establishment_id=$2 AND item_key=$3 AND reviewed_at IS NOT NULL",
                    &[&company_id, &establishment_id, &item_id],
                ).map_err(|_| FiscalError::Database)?;
                let Some(profile) = profile else {
                    return Ok(None);
                };
                if profile.get::<_, &str>(0) != item_kind {
                    return Ok(None);
                }
                match profile.get::<_, &str>(1) {
                    "customer" => customer_piece = true,
                    "workshop" => workshop_piece = true,
                    _ => {}
                }
            }
            if match expected_source {
                "none" => customer_piece || workshop_piece,
                "customer" => !customer_piece || workshop_piece,
                "workshop" => !workshop_piece || customer_piece,
                _ => true,
            } {
                return Ok(None);
            }
        }
        tx.commit().map_err(|_| FiscalError::Database)?;
        row.map(|r| {
            let json: String = r.get(5);
            let documents: Vec<DocumentPlan> =
                serde_json::from_str(&json).map_err(|_| FiscalError::Database)?;
            Ok(FiscalDecision {
                company_id,
                establishment_id,
                scenario: scenario.to_owned(),
                version: r.get(0),
                approved_by: r.get(1),
                approved_at_epoch: r.get(2),
                effective_from_epoch: r.get(3),
                effective_until_epoch: r.get(4),
                documents,
            })
        })
        .transpose()
    }
}
