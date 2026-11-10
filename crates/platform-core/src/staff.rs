//! Identidad y RBAC del personal interno de HiQNet (Control Center).
//!
//! Este módulo no conoce HTTP ni SQLx. No se mezcla con `commercial_account`
//! (clientes), con usuarios locales del POS ni con identidades de dispositivo.

use crate::ids::StaffAccountId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;

pub use crate::auth::{AuthError as StaffError, AuthResult as StaffResult};

/// Roles internos. Son presets de capabilities, no permisos ad-hoc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffRole {
    SuperAdmin,
    PlatformAdmin,
    Operator,
    Support,
    SecurityAuditor,
    ReadOnly,
}

impl StaffRole {
    pub const ALL: [StaffRole; 6] = [
        StaffRole::SuperAdmin,
        StaffRole::PlatformAdmin,
        StaffRole::Operator,
        StaffRole::Support,
        StaffRole::SecurityAuditor,
        StaffRole::ReadOnly,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StaffRole::SuperAdmin => "super_admin",
            StaffRole::PlatformAdmin => "platform_admin",
            StaffRole::Operator => "operator",
            StaffRole::Support => "support",
            StaffRole::SecurityAuditor => "security_auditor",
            StaffRole::ReadOnly => "read_only",
        }
    }

    /// Capabilities concedidas por el rol. Deny by default: lo ausente se niega.
    pub fn capabilities(self) -> &'static [Capability] {
        use Capability::*;
        const READ_ALL: [Capability; 8] = [
            ProductsRead,
            OrganizationsRead,
            AccountsRead,
            InstallationsRead,
            SubscriptionsRead,
            DeploymentsRead,
            SecurityRead,
            AuditRead,
        ];
        match self {
            StaffRole::SuperAdmin => &[
                ProductsRead,
                ProductsManage,
                OrganizationsRead,
                OrganizationsManage,
                AccountsRead,
                AccountsManage,
                InstallationsRead,
                InstallationsManage,
                InstallationsRehost,
                SubscriptionsRead,
                SubscriptionsManage,
                DeploymentsRead,
                DeploymentsManage,
                SecurityRead,
                SecurityManage,
                AuditRead,
                StaffManage,
            ],
            StaffRole::PlatformAdmin => &[
                ProductsRead,
                ProductsManage,
                OrganizationsRead,
                OrganizationsManage,
                AccountsRead,
                AccountsManage,
                InstallationsRead,
                InstallationsManage,
                InstallationsRehost,
                SubscriptionsRead,
                SubscriptionsManage,
                DeploymentsRead,
                DeploymentsManage,
                SecurityRead,
                AuditRead,
            ],
            StaffRole::Operator => &[
                ProductsRead,
                OrganizationsRead,
                AccountsRead,
                InstallationsRead,
                InstallationsManage,
                SubscriptionsRead,
                DeploymentsRead,
                DeploymentsManage,
                SecurityRead,
                AuditRead,
            ],
            StaffRole::Support => &READ_ALL,
            StaffRole::SecurityAuditor => &[
                ProductsRead,
                OrganizationsRead,
                AccountsRead,
                InstallationsRead,
                SubscriptionsRead,
                DeploymentsRead,
                SecurityRead,
                SecurityManage,
                AuditRead,
            ],
            StaffRole::ReadOnly => &READ_ALL,
        }
    }

    pub fn has(self, capability: Capability) -> bool {
        self.capabilities().contains(&capability)
    }
}

impl std::fmt::Display for StaffRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rol staff desconocido")]
pub struct StaffRoleParseError;

impl FromStr for StaffRole {
    type Err = StaffRoleParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "super_admin" => Ok(StaffRole::SuperAdmin),
            "platform_admin" => Ok(StaffRole::PlatformAdmin),
            "operator" => Ok(StaffRole::Operator),
            "support" => Ok(StaffRole::Support),
            "security_auditor" => Ok(StaffRole::SecurityAuditor),
            "read_only" => Ok(StaffRole::ReadOnly),
            _ => Err(StaffRoleParseError),
        }
    }
}

/// Capacidades granulares. Persistencia futura podrá concederlas explícitamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ProductsRead,
    ProductsManage,
    OrganizationsRead,
    OrganizationsManage,
    AccountsRead,
    AccountsManage,
    InstallationsRead,
    InstallationsManage,
    InstallationsRehost,
    SubscriptionsRead,
    SubscriptionsManage,
    DeploymentsRead,
    DeploymentsManage,
    SecurityRead,
    SecurityManage,
    AuditRead,
    StaffManage,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::ProductsRead => "products.read",
            Capability::ProductsManage => "products.manage",
            Capability::OrganizationsRead => "organizations.read",
            Capability::OrganizationsManage => "organizations.manage",
            Capability::AccountsRead => "accounts.read",
            Capability::AccountsManage => "accounts.manage",
            Capability::InstallationsRead => "installations.read",
            Capability::InstallationsManage => "installations.manage",
            Capability::InstallationsRehost => "installations.rehost",
            Capability::SubscriptionsRead => "subscriptions.read",
            Capability::SubscriptionsManage => "subscriptions.manage",
            Capability::DeploymentsRead => "deployments.read",
            Capability::DeploymentsManage => "deployments.manage",
            Capability::SecurityRead => "security.read",
            Capability::SecurityManage => "security.manage",
            Capability::AuditRead => "audit.read",
            Capability::StaffManage => "staff.manage",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffIdentity {
    pub account_id: StaffAccountId,
    pub email: String,
    pub display_name: String,
    pub role: StaffRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffCredential {
    pub account_id: StaffAccountId,
    pub password_hash: String,
    pub password_changed_at: OffsetDateTime,
}

impl StaffCredential {
    pub fn new(account_id: StaffAccountId, password_hash: String, now: OffsetDateTime) -> Self {
        Self {
            account_id,
            password_hash,
            password_changed_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedStaffSession {
    pub session_id: uuid::Uuid,
    pub account_id: StaffAccountId,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// Sesión recién emitida. El token sólo se entrega al cliente una vez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedStaffSession {
    pub token: String,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negar_por_defecto_y_capabilities_por_rol() {
        assert!(StaffRole::SuperAdmin.has(Capability::StaffManage));
        assert!(StaffRole::SuperAdmin.has(Capability::SecurityManage));
        assert!(!StaffRole::PlatformAdmin.has(Capability::StaffManage));
        assert!(!StaffRole::PlatformAdmin.has(Capability::SecurityManage));
        assert!(StaffRole::Operator.has(Capability::InstallationsManage));
        assert!(!StaffRole::Operator.has(Capability::OrganizationsManage));
        assert!(!StaffRole::Support.has(Capability::InstallationsManage));
        assert!(StaffRole::SecurityAuditor.has(Capability::SecurityManage));
        assert!(!StaffRole::SecurityAuditor.has(Capability::AccountsManage));
        for read in [
            Capability::ProductsRead,
            Capability::OrganizationsRead,
            Capability::AccountsRead,
            Capability::InstallationsRead,
            Capability::AuditRead,
        ] {
            assert!(StaffRole::ReadOnly.has(read));
        }
        assert!(!StaffRole::ReadOnly.has(Capability::ProductsManage));
    }

    #[test]
    fn roles_sobreviven_al_texto() {
        for role in StaffRole::ALL {
            let parsed: StaffRole = role.as_str().parse().unwrap();
            assert_eq!(parsed, role);
            assert_eq!(role.to_string(), role.as_str());
        }
        assert!("superuser".parse::<StaffRole>().is_err());
    }

    #[test]
    fn capabilities_tienen_nombres_estables() {
        assert_eq!(Capability::StaffManage.as_str(), "staff.manage");
        assert_eq!(Capability::InstallationsRead.as_str(), "installations.read");
    }
}
