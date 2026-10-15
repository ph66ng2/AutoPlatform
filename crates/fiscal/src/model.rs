use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Nfse,
    Nfe,
}

impl DocumentKind {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "nfse" => Some(Self::Nfse),
            "nfe" => Some(Self::Nfe),
            _ => None,
        }
    }
}

impl DocumentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nfse => "nfse",
            Self::Nfe => "nfe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentState {
    Prepared,
    Sending,
    Authorized,
    Rejected,
    Indeterminate,
    CancelPending,
    Cancelled,
}

impl DocumentState {
    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "prepared" => Self::Prepared,
            "sending" => Self::Sending,
            "authorized" => Self::Authorized,
            "rejected" => Self::Rejected,
            "indeterminate" => Self::Indeterminate,
            "cancel_pending" => Self::CancelPending,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Sending => "sending",
            Self::Authorized => "authorized",
            Self::Rejected => "rejected",
            Self::Indeterminate => "indeterminate",
            Self::CancelPending => "cancel_pending",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Prepared, Self::Sending)
                | (
                    Self::Sending,
                    Self::Authorized | Self::Rejected | Self::Indeterminate
                )
                | (
                    Self::Indeterminate,
                    Self::Authorized | Self::Rejected | Self::Cancelled
                )
                | (Self::Authorized, Self::CancelPending)
                | (
                    Self::CancelPending,
                    Self::Authorized | Self::Cancelled | Self::Indeterminate
                )
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPlan {
    /// Chave estável do documento dentro do fato; evita duplicatas concorrentes.
    pub key: String,
    pub kind: DocumentKind,
    /// Valor informado pela decisão, nunca calculado a partir do kind do item.
    pub amount_cents: i64,
    pub tax_base_cents: i64,
    pub fiscal_code: String,
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FiscalDecision {
    pub company_id: Uuid,
    pub establishment_id: Uuid,
    pub scenario: String,
    pub version: i64,
    pub approved_by: Uuid,
    pub approved_at_epoch: i64,
    pub effective_from_epoch: i64,
    pub effective_until_epoch: Option<i64>,
    pub documents: Vec<DocumentPlan>,
}

/// AP-FISC-CONFIG-001 implementará este resolvedor sobre decisões persistidas.
/// O consumidor não pode fornecer uma decisão arbitrária em uma chamada HTTP.
pub trait DecisionResolver {
    fn approved_decision(
        &self,
        company_id: Uuid,
        establishment_id: Uuid,
        scenario: &str,
        at_epoch: i64,
    ) -> Result<Option<FiscalDecision>, FiscalError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDocument {
    pub id: Uuid,
    pub company_id: Uuid,
    pub fact_id: Uuid,
    pub plan_key: String,
    pub kind: DocumentKind,
    pub state: DocumentState,
}

/// A confirmação humana identifica quem solicitou emissão ou cancelamento.
#[derive(Debug, Clone, Copy)]
pub struct HumanConfirmation {
    pub actor_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub company_id: Uuid,
    pub document_id: Uuid,
    pub kind: DocumentKind,
    pub fiscal_code: String,
    pub amount_cents: i64,
    pub tax_base_cents: i64,
    pub fact: serde_json::Value,
    /// Chave estável para consulta e idempotência no provedor.
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderOutcome {
    Authorized { xml: Vec<u8> },
    Rejected,
    Cancelled,
    Indeterminate,
}

/// Portas NFS-e/NF-e. Implementações concretas pertencem ao ticket de adapter.
pub trait FiscalProvider {
    fn issue(&self, request: &ProviderRequest) -> ProviderOutcome;
    fn cancel(&self, request: &ProviderRequest) -> ProviderOutcome;
    fn consult(&self, request: &ProviderRequest) -> ProviderOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiscalError {
    Denied,
    InvalidFact,
    DecisionMissing,
    DecisionNotApplicable,
    ValuesMissing,
    Conflict,
    NotFound,
    InvalidTransition,
    XmlRequired,
    Database,
}

impl std::fmt::Display for FiscalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Denied => "permissão FISCAL necessária",
            Self::InvalidFact => "fato comercial inválido",
            Self::DecisionMissing => "decisão fiscal aprovada ausente",
            Self::DecisionNotApplicable => "decisão fiscal fora do escopo ou vigência",
            Self::ValuesMissing => "valores fiscais explícitos ausentes ou inconsistentes",
            Self::Conflict => "snapshot ou plano conflitante",
            Self::NotFound => "documento não encontrado",
            Self::InvalidTransition => "transição fiscal inválida",
            Self::XmlRequired => "XML obrigatório para autorização",
            Self::Database => "falha no armazenamento fiscal",
        })
    }
}
