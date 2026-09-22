//! Fronteira do módulo `receivables`.
//!
//! Contas a receber e títulos da venda. Não conversa com o banco nem com o gateway.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("receivables");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
