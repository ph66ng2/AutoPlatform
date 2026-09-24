use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Availability {
    pub sku: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
}

impl Availability {
    pub fn new(on_hand: i64, reserved: i64) -> Result<Self, InventoryError> {
        if on_hand < 0 || reserved < 0 || on_hand < reserved {
            return Err(InventoryError::Invariant);
        }
        Ok(Self {
            sku: String::new(),
            on_hand,
            reserved,
            available: on_hand - reserved,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    Denied,
    Offline,
    Insufficient,
    NotFound,
    Conflict,
    Invariant,
    Protocol,
}

impl std::fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Denied => "acesso negado",
            Self::Offline => "mutação offline de estoque é proibida",
            Self::Insufficient => "estoque insuficiente",
            Self::NotFound => "sku ou reserva ausente",
            Self::Conflict => "operation_id com outro comando",
            Self::Invariant => "saldo negativo ou reserva acima de on_hand",
            Self::Protocol => "comando de estoque recusado",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CommandOutcome {
    pub operation_id: String,
    pub sku: String,
    pub quantity: i32,
    pub reason: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
    pub movement_id: Option<String>,
    pub reservation_id: Option<String>,
    pub replay: bool,
}
