//! Fronteira do módulo `integration`.
//!
//! Outbox, inbox e contratos entre AutoOS e AutoBO. Não escreve nas tabelas internas do outro produto.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("integration");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
