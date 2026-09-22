//! Fronteira do módulo `banking`.
//!
//! Sicredi e credenciais bancárias no servidor. Não implementa o gateway de cartão da oficina.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("banking");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
