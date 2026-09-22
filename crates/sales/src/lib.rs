//! Fronteira do módulo `sales`.
//!
//! Venda standalone, itens, totais e estados comercial, pagamento, entrega e fiscal independentes. O histórico guarda snapshot.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("sales");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
