//! Suite do gate de fundação. Não declara Go. Falha crítica impede o avanço.

ap_kernel::declare_module!("security-tests");

#[cfg(test)]
mod tests {
    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
    }
}
