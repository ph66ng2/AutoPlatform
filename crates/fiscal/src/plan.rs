use std::collections::{HashMap, HashSet};

use ap_contracts::{load_fato_comercial_v2, validate_fato_comercial_v2};
use ap_identity::{Permission, Session};
use serde_json::Value;
use uuid::Uuid;

use crate::model::{DecisionResolver, FiscalDecision, FiscalError};

#[derive(Debug)]
pub struct ValidatedPlan {
    pub fact_id: Uuid,
    pub os_id: Uuid,
    pub os_version: i64,
    pub company_id: Uuid,
    pub establishment_id: Uuid,
    pub decision: FiscalDecision,
}

pub fn prepare_plan(
    session: Session,
    fact: &Value,
    scenario: &str,
    at_epoch: i64,
    resolver: &impl DecisionResolver,
) -> Result<ValidatedPlan, FiscalError> {
    if !session.allows(Permission::Fiscal) {
        return Err(FiscalError::Denied);
    }
    let schema = load_fato_comercial_v2().map_err(|_| FiscalError::InvalidFact)?;
    validate_fato_comercial_v2(&schema, fact).map_err(|_| FiscalError::InvalidFact)?;
    if fact["origin"] != "autoos" || fact["status"] != "confirmed" {
        return Err(FiscalError::InvalidFact);
    }
    let company_id = uuid(fact, "company_id")?;
    let establishment_id = uuid(fact, "establishment_id")?;
    let fact_id = uuid(fact, "fact_id")?;
    let os_id = uuid(fact, "os_id")?;
    let os_version = fact["os_version"]
        .as_i64()
        .ok_or(FiscalError::InvalidFact)?;
    if company_id != session.company_id || scenario.is_empty() {
        return Err(FiscalError::Denied);
    }
    let decision = resolver
        .approved_decision(company_id, establishment_id, scenario, at_epoch)?
        .ok_or(FiscalError::DecisionMissing)?;
    validate_decision(
        &decision,
        fact,
        company_id,
        establishment_id,
        scenario,
        at_epoch,
    )?;
    Ok(ValidatedPlan {
        fact_id,
        os_id,
        os_version,
        company_id,
        establishment_id,
        decision,
    })
}

fn uuid(fact: &Value, field: &str) -> Result<Uuid, FiscalError> {
    fact[field]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(FiscalError::InvalidFact)
}

fn validate_decision(
    decision: &FiscalDecision,
    fact: &Value,
    company_id: Uuid,
    establishment_id: Uuid,
    scenario: &str,
    at_epoch: i64,
) -> Result<(), FiscalError> {
    if decision.company_id != company_id
        || decision.establishment_id != establishment_id
        || decision.scenario != scenario
        || decision.version <= 0
        || decision.approved_by.is_nil()
        || decision.approved_at_epoch <= 0
        || decision.approved_at_epoch > at_epoch
        || decision.effective_from_epoch > at_epoch
        || decision
            .effective_until_epoch
            .is_some_and(|until| at_epoch >= until)
    {
        return Err(FiscalError::DecisionNotApplicable);
    }
    if decision.documents.is_empty() {
        return Err(FiscalError::ValuesMissing);
    }
    let item_totals: HashMap<&str, i64> = fact["items"]
        .as_array()
        .ok_or(FiscalError::InvalidFact)?
        .iter()
        .map(|item| {
            (
                item["item_id"].as_str().unwrap_or_default(),
                item["total_cents"].as_i64().unwrap_or_default(),
            )
        })
        .collect();
    let itemized = fact["pricing_mode"] == "itemized";
    let mut seen_items = HashSet::new();
    let mut seen_keys = HashSet::new();
    let mut planned_total = 0_i64;
    for doc in &decision.documents {
        if doc.key.is_empty()
            || doc.key.len() > 64
            || doc.fiscal_code.is_empty()
            || doc.amount_cents < 0
            || doc.tax_base_cents < 0
            || doc.item_ids.is_empty()
            || !seen_keys.insert(&doc.key)
        {
            return Err(FiscalError::ValuesMissing);
        }
        let mut item_total = 0_i64;
        for item_id in &doc.item_ids {
            let Some(amount) = item_totals.get(item_id.as_str()) else {
                return Err(FiscalError::ValuesMissing);
            };
            if !seen_items.insert(item_id) {
                return Err(FiscalError::ValuesMissing);
            }
            item_total = item_total
                .checked_add(*amount)
                .ok_or(FiscalError::ValuesMissing)?;
        }
        if itemized && item_total != doc.amount_cents {
            return Err(FiscalError::ValuesMissing);
        }
        planned_total = planned_total
            .checked_add(doc.amount_cents)
            .ok_or(FiscalError::ValuesMissing)?;
    }
    if seen_items.len() != item_totals.len()
        || Some(planned_total) != fact["totals"]["total_cents"].as_i64()
    {
        return Err(FiscalError::ValuesMissing);
    }
    Ok(())
}
