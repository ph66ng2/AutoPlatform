//! Módulos ligados pelos dois binários. A lista é única para API e worker.

pub const NAMES: &[&str] = &[
    ap_audit::MODULE_NAME,
    ap_banking::MODULE_NAME,
    ap_billing::MODULE_NAME,
    ap_entitlements::MODULE_NAME,
    ap_fiscal::MODULE_NAME,
    ap_identity::MODULE_NAME,
    ap_integration::MODULE_NAME,
    ap_inventory::MODULE_NAME,
    ap_payments::MODULE_NAME,
    ap_portal::MODULE_NAME,
    ap_receivables::MODULE_NAME,
    ap_sales::MODULE_NAME,
];

#[cfg(test)]
mod tests {
    #[test]
    fn matches_the_required_boundaries() {
        assert_eq!(super::NAMES, ap_kernel::REQUIRED_MODULES);
    }
}
