//! Tipos mínimos compartilhados pelas fronteiras da plataforma.
//!
//! Domínios dependem deste crate. Ele não depende de domínio, API ou worker.

/// Fronteira nomeada de um módulo de domínio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleBoundary {
    /// Nome estável do módulo, usado pela composição da API e do worker.
    pub name: &'static str,
}

/// Módulos que a API e o worker precisam ligar, sem depender um do outro.
pub const REQUIRED_MODULES: &[&str] = &[
    "audit",
    "banking",
    "billing",
    "entitlements",
    "fiscal",
    "identity",
    "integration",
    "inventory",
    "payments",
    "portal",
    "receivables",
    "sales",
];

/// Declara o nome público de um módulo de domínio.
#[macro_export]
macro_rules! declare_module {
    ($name:literal) => {
        pub const MODULE_NAME: &str = $name;

        /// Retorna a fronteira deste módulo.
        #[must_use]
        pub const fn boundary() -> $crate::ModuleBoundary {
            $crate::ModuleBoundary { name: MODULE_NAME }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::REQUIRED_MODULES;

    #[test]
    fn required_modules_are_unique_and_sorted() {
        let mut sorted = REQUIRED_MODULES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, REQUIRED_MODULES);
    }
}
