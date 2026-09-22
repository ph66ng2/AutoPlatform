//! Fronteira do módulo `billing`.
//!
//! Assinatura da plataforma, faturas e estados normalizados do SaaS. Não cobra a oficina.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("billing");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
