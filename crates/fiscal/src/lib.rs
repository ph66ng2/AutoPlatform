//! Núcleo fiscal provider-neutral para fatos faturáveis v2 de OS.
//!
//! Não escolhe regra tributária, não emite automaticamente e não depende de
//! Sale, estoque ou de tabelas de documentos fiscais importados.

mod engine;
mod model;
mod plan;
mod store;

pub use engine::{cancel, consult, issue};
pub use model::{
    DecisionResolver, DocumentKind, DocumentPlan, DocumentState, FiscalDecision, FiscalError,
    FiscalProvider, HumanConfirmation, PreparedDocument, ProviderOutcome, ProviderRequest,
};
pub use plan::prepare_plan;
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
