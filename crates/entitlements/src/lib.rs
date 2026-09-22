//! Fronteira do módulo `entitlements`.
//!
//! Capacidades por produto, validade e entitlement assinado. Não confirma pagamento nem ativa acesso por redirect.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("entitlements");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
