//! Fronteira do módulo `payments`.
//!
//! PaymentIntent, tentativas, estorno e chargeback da oficina. Redirect de checkout não confirma pagamento.
//! Este crate não referencia outros domínios.

ap_kernel::declare_module!("payments");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }
}
