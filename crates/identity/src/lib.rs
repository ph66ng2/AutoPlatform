//! Fronteira do módulo `identity`.
//!
//! Company e papel vêm da membership ativa. `user_metadata`, `app_metadata` e
//! `company_id` dentro do JWT não autorizam. ADMIN AutoOS não vira FISCAL AutoBO.
//! Este crate não referencia outros domínios.

mod directory;
mod jwt;
mod model;

use uuid::Uuid;

use jwt::account_id;

pub use directory::{
    AuditEntry, Directory, MembershipRejected, MembershipStatus, MemoryDirectory, PartyStatus,
};
pub use jwt::JwtSecret;
pub use model::{Permission, Product, Role};

ap_kernel::declare_module!("identity");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessError {
    Unauthenticated,
    Denied,
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unauthenticated => "sessão inválida",
            Self::Denied => "acesso negado",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub account_id: Uuid,
    pub company_id: Uuid,
    pub product: Product,
    pub role: Role,
}

impl Session {
    #[must_use]
    pub const fn allows(self, permission: Permission) -> bool {
        self.role.grants(self.product, permission)
    }
}

pub fn resolve(
    secret: &JwtSecret,
    directory: &impl Directory,
    token: &str,
    company_id: Uuid,
    product: Product,
) -> Result<Session, AccessError> {
    let account_id = account_id(secret, token)?;
    let Some(membership) = directory.active_membership(account_id, company_id, product) else {
        return Err(AccessError::Denied);
    };
    Ok(Session {
        account_id: membership.account_id,
        company_id: membership.company_id,
        product: membership.product,
        role: membership.role,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use uuid::Uuid;

    use super::{
        resolve, AccessError, JwtSecret, MembershipStatus, MemoryDirectory, PartyStatus,
        Permission, Product, Role,
    };

    const SECRET: &[u8] = b"synthetic-jwt-hs256-key";

    fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(0xa1),
            Uuid::from_u128(0xb1),
            Uuid::from_u128(0xa2),
            Uuid::from_u128(0xb2),
        )
    }

    fn token(account: Uuid, company_claim: Uuid) -> String {
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("relógio")
            .as_secs()
            + 600;
        let claims = json!({
            "sub": account.to_string(),
            "aud": "authenticated",
            "exp": exp,
            "role": "authenticated",
            "user_metadata": {"company_id": company_claim.to_string(), "role": "fiscal"},
            "app_metadata": {"company_id": company_claim.to_string(), "role": "fiscal"},
            "company_id": company_claim.to_string(),
        });
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET),
        )
        .expect("jwt")
    }

    fn directory() -> (MemoryDirectory, Uuid, Uuid, Uuid, Uuid, Uuid) {
        let (company_a, company_b, account_a, account_b) = ids();
        let mut directory = MemoryDirectory::default();
        directory.insert_company(company_a, PartyStatus::Active);
        directory.insert_company(company_b, PartyStatus::Active);
        directory.insert_account(account_a, PartyStatus::Active);
        directory.insert_account(account_b, PartyStatus::Active);
        let autoos = Uuid::from_u128(0xa3);
        let autobo_admin = Uuid::from_u128(0xa4);
        directory
            .insert_membership(
                autoos,
                company_a,
                account_a,
                Product::AutoOs,
                Role::Admin,
                MembershipStatus::Active,
            )
            .expect("autoos");
        directory
            .insert_membership(
                autobo_admin,
                company_a,
                account_a,
                Product::AutoBo,
                Role::Admin,
                MembershipStatus::Active,
            )
            .expect("autobo admin");
        directory
            .insert_membership(
                Uuid::from_u128(0xb3),
                company_b,
                account_b,
                Product::AutoBo,
                Role::Fiscal,
                MembershipStatus::Active,
            )
            .expect("fiscal b");
        (
            directory,
            company_a,
            company_b,
            account_a,
            autoos,
            autobo_admin,
        )
    }

    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }

    #[test]
    fn autoos_admin_does_not_receive_autobo_fiscal() {
        let (directory, company_a, company_b, account_a, _, _) = directory();
        let secret = JwtSecret::new(SECRET.to_vec());
        let minted = token(account_a, company_b);
        let autoos =
            resolve(&secret, &directory, &minted, company_a, Product::AutoOs).expect("sessão");
        assert!(autoos.allows(Permission::ManageCompany));
        assert!(!autoos.allows(Permission::Fiscal));
        let autobo =
            resolve(&secret, &directory, &minted, company_a, Product::AutoBo).expect("sessão");
        assert!(autobo.allows(Permission::ManageCompany));
        assert!(!autobo.allows(Permission::Fiscal));
        let fiscal = resolve(
            &secret,
            &directory,
            &token(ids().3, company_a),
            company_b,
            Product::AutoBo,
        )
        .expect("fiscal");
        assert!(fiscal.allows(Permission::Fiscal));
        assert!(!fiscal.allows(Permission::ManageCompany));
    }

    #[test]
    fn arbitrary_company_id_is_denied_like_any_other_miss() {
        let (mut directory, company_a, company_b, account_a, autoos, autobo_admin) = directory();
        let secret = JwtSecret::new(SECRET.to_vec());
        let minted = token(account_a, company_b);
        let unknown = Uuid::from_u128(0xdead);
        let cross = resolve(&secret, &directory, &minted, company_b, Product::AutoOs);
        let missing = resolve(&secret, &directory, &minted, unknown, Product::AutoOs);
        assert_eq!(cross, Err(AccessError::Denied));
        assert_eq!(missing, Err(AccessError::Denied));
        assert_eq!(format!("{cross:?}"), format!("{missing:?}"));

        directory
            .set_membership_status(company_a, autoos, MembershipStatus::Suspended)
            .expect("suspende");
        directory
            .set_membership_status(company_a, autobo_admin, MembershipStatus::Revoked)
            .expect("revoga");
        let suspended = resolve(&secret, &directory, &minted, company_a, Product::AutoOs);
        let revoked = resolve(&secret, &directory, &minted, company_a, Product::AutoBo);
        assert_eq!(suspended, Err(AccessError::Denied));
        assert_eq!(revoked, Err(AccessError::Denied));
        let actions: Vec<&str> = directory
            .audits()
            .iter()
            .map(|entry| entry.action)
            .collect();
        assert_eq!(actions, ["membership_suspended", "membership_revoked"]);
        assert!(directory
            .audits()
            .iter()
            .all(|entry| entry.company_id == company_a && entry.outcome == "success"));
    }

    #[test]
    fn rejects_autoos_fiscal_and_service_role_token() {
        let (company_a, _, account_a, _) = ids();
        let mut directory = MemoryDirectory::default();
        let error = directory.insert_membership(
            Uuid::from_u128(1),
            company_a,
            account_a,
            Product::AutoOs,
            Role::Fiscal,
            MembershipStatus::Active,
        );
        assert!(error.is_err());

        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("relógio")
            .as_secs()
            + 600;
        let service = json!({
            "sub": account_a.to_string(),
            "aud": "authenticated",
            "exp": exp,
            "role": "service_role",
        });
        let minted = encode(
            &Header::default(),
            &service,
            &EncodingKey::from_secret(SECRET),
        )
        .expect("jwt");
        let resolved = resolve(
            &JwtSecret::new(SECRET.to_vec()),
            &directory,
            &minted,
            company_a,
            Product::AutoOs,
        );
        assert_eq!(resolved, Err(AccessError::Unauthenticated));
        assert!(!format!("{resolved:?}").contains(&account_a.to_string()));
        assert_eq!(
            format!("{:?}", JwtSecret::new(SECRET.to_vec())),
            "JwtSecret([REDACTED])"
        );
    }
}
