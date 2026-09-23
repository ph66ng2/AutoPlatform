/// Produto da membership. Um papel não atravessa esta fronteira.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    AutoOs,
    AutoBo,
}

impl Product {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutoOs => "autoos",
            Self::AutoBo => "autobo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    Operator,
    Fiscal,
}

impl Role {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Fiscal => "fiscal",
        }
    }

    #[must_use]
    pub const fn fits(self, product: Product) -> bool {
        matches!(
            (product, self),
            (Product::AutoOs, Self::Admin | Self::Operator)
                | (Product::AutoBo, Self::Admin | Self::Operator | Self::Fiscal)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Operate,
    ManageCompany,
    Fiscal,
}

impl Role {
    #[must_use]
    pub const fn grants(self, product: Product, permission: Permission) -> bool {
        matches!(
            (product, self, permission),
            (
                Product::AutoOs | Product::AutoBo,
                Self::Admin,
                Permission::ManageCompany | Permission::Operate,
            ) | (
                Product::AutoOs | Product::AutoBo,
                Self::Operator,
                Permission::Operate,
            ) | (Product::AutoBo, Self::Fiscal, Permission::Fiscal)
        )
    }
}
