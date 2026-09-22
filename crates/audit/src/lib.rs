//! Fronteira do módulo `audit`.
//!
//! Eventos de auditoria e correlação. Não persiste segredo, documento integral nem payload sensível.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("audit");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
