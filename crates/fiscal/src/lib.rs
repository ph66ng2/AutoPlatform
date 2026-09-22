//! Fronteira do módulo `fiscal`.
//!
//! Perfis, documentos, eventos e máquina de estados fiscal sem provider. Não emite sozinho nem escolhe adapter.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("fiscal");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
