//! Fronteira do módulo `identity`.
//!
//! Companies, memberships, papéis e sessão autorizada por produto. Não concede permissão de outro produto por implicação.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("identity");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
