//! Fronteira do módulo `portal`.
//!
//! Projeção pública, token opaco e aprovação versionada. Não enumera identificadores internos.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("portal");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
