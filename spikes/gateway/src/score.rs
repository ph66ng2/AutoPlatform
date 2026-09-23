use std::{collections::BTreeMap, fs};

use serde::Deserialize;

pub const PASS_SCORE: u8 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    PlatformBilling,
    WorkshopPayments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Asaas,
    MercadoPago,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Matrix {
    pub contexts: Vec<Context>,
    pub providers: Vec<Provider>,
    pub pass_score: u8,
    pub blockers: Vec<String>,
    pub weights: BTreeMap<String, u8>,
    pub selected: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scorecard {
    pub live_executed: bool,
    pub live_score: Option<u8>,
    pub synthetic_blockers: bool,
    pub selected: Option<Provider>,
}

pub fn load_matrix() -> Matrix {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/providers/matriz.json");
    let text = fs::read_to_string(path).expect("matriz");
    serde_json::from_str(&text).expect("json")
}

#[must_use]
pub fn evaluate(
    matrix: &Matrix,
    synthetic_blockers: bool,
) -> BTreeMap<(Context, Provider), Scorecard> {
    let mut cards = BTreeMap::new();
    for context in &matrix.contexts {
        for provider in &matrix.providers {
            let key = match context {
                Context::PlatformBilling => "platform_billing",
                Context::WorkshopPayments => "workshop_payments",
            };
            let selected = matrix
                .selected
                .get(key)
                .and_then(Option::as_deref)
                .and_then(|name| match name {
                    "asaas" => Some(Provider::Asaas),
                    "mercado_pago" => Some(Provider::MercadoPago),
                    _ => None,
                });
            cards.insert(
                (*context, *provider),
                Scorecard {
                    live_executed: false,
                    live_score: None,
                    synthetic_blockers,
                    selected,
                },
            );
        }
    }
    cards
}
