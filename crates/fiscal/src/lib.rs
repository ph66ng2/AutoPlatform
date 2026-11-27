//! Núcleo fiscal provider-neutral para fatos faturáveis v2 de OS.
//!
//! Não escolhe regra tributária, não emite automaticamente e não depende de
//! Sale, estoque ou de tabelas de documentos fiscais importados.

mod config;
mod engine;
mod model;
mod plan;
mod secrets;
mod store;

pub use config::{
    approve_decision, approve_item, approve_scenario, complete_issuer_profile,
    create_issuer_profile, register_item, register_scenario, save_decision_draft, DecisionApproval,
    PgDecisionResolver,
};
pub use engine::{cancel, consult, issue};
pub use model::{
    DecisionResolver, DocumentKind, DocumentPlan, DocumentState, FiscalDecision, FiscalError,
    FiscalProvider, HumanConfirmation, PreparedDocument, ProviderOutcome, ProviderRequest,
};
pub use plan::prepare_plan;
pub use secrets::{read_provider_secret, rotate_provider_secret, save_provider_secret, MasterKey};
pub use store::{prepare, transition};

ap_kernel::declare_module!("fiscal");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
