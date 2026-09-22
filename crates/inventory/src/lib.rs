//! Fronteira do módulo `inventory`.
//!
//! Catálogo operacional, ledger imutável, saldo e reservas. Saldo não é coluna editável e não fica negativo.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("inventory");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
