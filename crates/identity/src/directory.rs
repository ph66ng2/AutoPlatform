use std::fmt;

use uuid::Uuid;

use crate::model::{Product, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyStatus {
    Active,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipStatus {
    Active,
    Suspended,
    Revoked,
}

impl MembershipStatus {
    #[must_use]
    pub const fn audit_action(self) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Suspended => Some("membership_suspended"),
            Self::Revoked => Some("membership_revoked"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveMembership {
    pub company_id: Uuid,
    pub account_id: Uuid,
    pub product: Product,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub company_id: Uuid,
    pub account_id: Uuid,
    pub action: &'static str,
    pub outcome: &'static str,
}

pub trait Directory {
    fn active_membership(
        &self,
        account_id: Uuid,
        company_id: Uuid,
        product: Product,
    ) -> Option<ActiveMembership>;
}

#[derive(Debug, Clone)]
struct StoredMembership {
    id: Uuid,
    company_id: Uuid,
    account_id: Uuid,
    product: Product,
    role: Role,
    status: MembershipStatus,
}

#[derive(Debug, Default)]
pub struct MemoryDirectory {
    companies: Vec<(Uuid, PartyStatus)>,
    accounts: Vec<(Uuid, PartyStatus)>,
    memberships: Vec<StoredMembership>,
    audits: Vec<AuditEntry>,
}

impl MemoryDirectory {
    pub fn insert_company(&mut self, id: Uuid, status: PartyStatus) {
        self.companies.push((id, status));
    }

    pub fn insert_account(&mut self, id: Uuid, status: PartyStatus) {
        self.accounts.push((id, status));
    }

    pub fn insert_membership(
        &mut self,
        id: Uuid,
        company_id: Uuid,
        account_id: Uuid,
        product: Product,
        role: Role,
        status: MembershipStatus,
    ) -> Result<(), MembershipRejected> {
        if !role.fits(product) {
            return Err(MembershipRejected::RoleDoesNotFitProduct);
        }
        self.memberships.push(StoredMembership {
            id,
            company_id,
            account_id,
            product,
            role,
            status,
        });
        Ok(())
    }

    pub fn set_membership_status(
        &mut self,
        company_id: Uuid,
        membership_id: Uuid,
        status: MembershipStatus,
    ) -> Result<(), MembershipRejected> {
        let Some(action) = status.audit_action() else {
            return Err(MembershipRejected::StatusChangeUnsupported);
        };
        let Some(membership) = self.memberships.iter_mut().find(|membership| {
            membership.company_id == company_id && membership.id == membership_id
        }) else {
            return Err(MembershipRejected::NotInCompany);
        };
        membership.status = status;
        self.audits.push(AuditEntry {
            company_id,
            account_id: membership.account_id,
            action,
            outcome: "success",
        });
        Ok(())
    }

    #[must_use]
    pub fn audits(&self) -> &[AuditEntry] {
        &self.audits
    }
}

impl Directory for MemoryDirectory {
    fn active_membership(
        &self,
        account_id: Uuid,
        company_id: Uuid,
        product: Product,
    ) -> Option<ActiveMembership> {
        let company_active = self
            .companies
            .iter()
            .any(|(id, status)| *id == company_id && *status == PartyStatus::Active);
        let account_active = self
            .accounts
            .iter()
            .any(|(id, status)| *id == account_id && *status == PartyStatus::Active);
        if !company_active || !account_active {
            return None;
        }
        self.memberships.iter().find_map(|membership| {
            (membership.company_id == company_id
                && membership.account_id == account_id
                && membership.product == product
                && membership.status == MembershipStatus::Active)
                .then_some(ActiveMembership {
                    company_id,
                    account_id,
                    product,
                    role: membership.role,
                })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipRejected {
    RoleDoesNotFitProduct,
    StatusChangeUnsupported,
    NotInCompany,
}

impl fmt::Display for MembershipRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RoleDoesNotFitProduct => "papel não pertence ao produto",
            Self::StatusChangeUnsupported => "mudança de status não auditável",
            Self::NotInCompany => "membership não pertence à company",
        })
    }
}
