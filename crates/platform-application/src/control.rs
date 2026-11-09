//! Lecturas administrativas globales del Control Center.
//!
//! Proyecciones read-only sobre organization, commercial_account y sus
//! relaciones. No reutiliza la autoridad comercial: comparte almacenamiento,
//! no casos de uso ni reglas de acceso del Customer Portal.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::{
    ids::{CommercialAccountId, OrganizationId, ProductId, StaffAccountId},
    product::LifecycleStatus,
    staff::{Capability, StaffRole},
};
use serde_json::Value;
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    #[error("recurso no encontrado")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("parámetros de consulta inválidos")]
    InvalidInput,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type ControlResult<T> = Result<T, ControlError>;

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 100;
pub const MAX_SEARCH_LEN: usize = 200;
pub const MAX_CURSOR_LEN: usize = 128;

macro_rules! read_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl FromStr for $name {
            type Err = ControlError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(ControlError::Storage),
                }
            }
        }
    };
}

read_enum!(OrganizationStatus {
    Active => "active",
    LegacyUnclaimed => "legacy_unclaimed",
    Archived => "archived",
});

read_enum!(AccountStatus {
    Active => "active",
    Disabled => "disabled",
});

read_enum!(MembershipRole {
    Owner => "owner",
    Admin => "admin",
    Member => "member",
});

read_enum!(AssociationStatus {
    Active => "active",
    Inactive => "inactive",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlOrganizationSummary {
    pub id: OrganizationId,
    pub display_name: String,
    pub status: OrganizationStatus,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlMembershipView {
    pub account_id: CommercialAccountId,
    pub email: String,
    pub display_name: String,
    pub account_status: AccountStatus,
    pub role: MembershipRole,
    pub joined_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlOrganizationProductView {
    pub product_id: ProductId,
    pub product_key: String,
    pub product_name: String,
    pub product_status: LifecycleStatus,
    pub association_status: AssociationStatus,
    pub activated_at: OffsetDateTime,
    pub entitlements: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlOrganizationDetail {
    pub organization: ControlOrganizationSummary,
    pub memberships: Vec<ControlMembershipView>,
    pub products: Vec<ControlOrganizationProductView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlAccountSummary {
    pub id: CommercialAccountId,
    pub email: String,
    pub display_name: String,
    pub status: AccountStatus,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlAccountMembershipView {
    pub organization_id: OrganizationId,
    pub organization_display_name: String,
    pub organization_status: OrganizationStatus,
    pub role: MembershipRole,
    pub joined_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlAccountDetail {
    pub account: ControlAccountSummary,
    pub memberships: Vec<ControlAccountMembershipView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlCursor {
    pub created_at: OffsetDateTime,
    pub id: Uuid,
}

impl ControlCursor {
    pub fn encode(self) -> String {
        URL_SAFE_NO_PAD.encode(format!(
            "{}:{}",
            self.created_at.unix_timestamp_nanos(),
            self.id
        ))
    }

    pub fn decode(value: &str) -> ControlResult<Self> {
        if value.is_empty() || value.len() > MAX_CURSOR_LEN {
            return Err(ControlError::InvalidInput);
        }
        let decoded = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| ControlError::InvalidInput)?;
        let text = String::from_utf8(decoded).map_err(|_| ControlError::InvalidInput)?;
        let (timestamp, id) = text.split_once(':').ok_or(ControlError::InvalidInput)?;
        let nanos: i128 = timestamp.parse().map_err(|_| ControlError::InvalidInput)?;
        let created_at = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| ControlError::InvalidInput)?;
        let id = Uuid::parse_str(id).map_err(|_| ControlError::InvalidInput)?;
        Ok(Self { created_at, id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait ControlRepository: Send + Sync {
    async fn list_organizations(
        &self,
        status: Option<OrganizationStatus>,
        search: Option<&str>,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> ControlResult<Vec<ControlOrganizationSummary>>;
    async fn get_organization(
        &self,
        id: OrganizationId,
    ) -> ControlResult<Option<ControlOrganizationDetail>>;
    async fn list_accounts(
        &self,
        status: Option<AccountStatus>,
        search: Option<&str>,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> ControlResult<Vec<ControlAccountSummary>>;
    async fn get_account(
        &self,
        id: CommercialAccountId,
    ) -> ControlResult<Option<ControlAccountDetail>>;
}

#[derive(Debug, Clone, Copy)]
pub struct ControlActor {
    pub account_id: StaffAccountId,
    capabilities: &'static [Capability],
}

impl ControlActor {
    pub fn new(account_id: StaffAccountId, role: StaffRole) -> Self {
        Self {
            account_id,
            capabilities: role.capabilities(),
        }
    }

    pub fn require(&self, capability: Capability) -> ControlResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(ControlError::Forbidden)
        }
    }
}

pub struct ControlService<R: ControlRepository> {
    repository: R,
}

impl<R: ControlRepository> ControlService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list_organizations(
        &self,
        actor: &ControlActor,
        status: Option<OrganizationStatus>,
        search: Option<String>,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> ControlResult<Page<ControlOrganizationSummary>> {
        actor.require(Capability::OrganizationsRead)?;
        let (limit, after) = page_params(limit, cursor)?;
        let search = normalize_search(search)?;
        let mut items = self
            .repository
            .list_organizations(status, search.as_deref(), limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, |item| ControlCursor {
            created_at: item.created_at,
            id: item.id.as_uuid(),
        });
        Ok(Page { items, next_cursor })
    }

    pub async fn get_organization(
        &self,
        actor: &ControlActor,
        id: OrganizationId,
    ) -> ControlResult<ControlOrganizationDetail> {
        actor.require(Capability::OrganizationsRead)?;
        self.repository
            .get_organization(id)
            .await?
            .ok_or(ControlError::NotFound)
    }

    pub async fn list_accounts(
        &self,
        actor: &ControlActor,
        status: Option<AccountStatus>,
        search: Option<String>,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> ControlResult<Page<ControlAccountSummary>> {
        actor.require(Capability::AccountsRead)?;
        let (limit, after) = page_params(limit, cursor)?;
        let search = normalize_search(search)?;
        let mut items = self
            .repository
            .list_accounts(status, search.as_deref(), limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, |item| ControlCursor {
            created_at: item.created_at,
            id: item.id.as_uuid(),
        });
        Ok(Page { items, next_cursor })
    }

    pub async fn get_account(
        &self,
        actor: &ControlActor,
        id: CommercialAccountId,
    ) -> ControlResult<ControlAccountDetail> {
        actor.require(Capability::AccountsRead)?;
        self.repository
            .get_account(id)
            .await?
            .ok_or(ControlError::NotFound)
    }
}

fn page_params(
    limit: Option<u32>,
    cursor: Option<String>,
) -> ControlResult<(u32, Option<ControlCursor>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    let after = cursor
        .map(|value| ControlCursor::decode(&value))
        .transpose()?;
    Ok((limit, after))
}

fn normalize_search(search: Option<String>) -> ControlResult<Option<String>> {
    let Some(search) = search else {
        return Ok(None);
    };
    let trimmed = search.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_SEARCH_LEN {
        return Err(ControlError::InvalidInput);
    }
    Ok(Some(trimmed.to_owned()))
}

fn take_page_cursor<T>(
    items: &mut Vec<T>,
    limit: u32,
    cursor: impl Fn(&T) -> ControlCursor,
) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(|item| cursor(item).encode())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: StaffRole) -> ControlActor {
        ControlActor::new(StaffAccountId::new(), role)
    }

    #[test]
    fn capabilities_de_organizaciones_y_cuentas_son_independientes() {
        assert!(
            actor(StaffRole::Support)
                .require(Capability::OrganizationsRead)
                .is_ok()
        );
        assert!(
            actor(StaffRole::Support)
                .require(Capability::AccountsRead)
                .is_ok()
        );
        assert_eq!(
            actor(StaffRole::ReadOnly).require(Capability::OrganizationsManage),
            Err(ControlError::Forbidden)
        );

        let only_organizations = ControlActor {
            account_id: StaffAccountId::new(),
            capabilities: &[Capability::OrganizationsRead],
        };
        assert!(
            only_organizations
                .require(Capability::OrganizationsRead)
                .is_ok()
        );
        assert_eq!(
            only_organizations.require(Capability::AccountsRead),
            Err(ControlError::Forbidden)
        );

        let only_accounts = ControlActor {
            account_id: StaffAccountId::new(),
            capabilities: &[Capability::AccountsRead],
        };
        assert!(only_accounts.require(Capability::AccountsRead).is_ok());
        assert_eq!(
            only_accounts.require(Capability::OrganizationsRead),
            Err(ControlError::Forbidden)
        );
    }

    #[test]
    fn cursor_sobrevive_ida_y_vuelta_y_rechaza_basura() {
        let cursor = ControlCursor {
            created_at: OffsetDateTime::now_utc(),
            id: Uuid::new_v4(),
        };
        let encoded = cursor.encode();
        assert_eq!(ControlCursor::decode(&encoded).unwrap(), cursor);
        assert_eq!(
            ControlCursor::decode("no-es-un-cursor"),
            Err(ControlError::InvalidInput)
        );
        assert_eq!(
            ControlCursor::decode(&"x".repeat(MAX_CURSOR_LEN + 1)),
            Err(ControlError::InvalidInput)
        );
    }

    #[test]
    fn busqueda_se_normaliza_y_se_acota() {
        assert_eq!(
            normalize_search(Some("  pos  ".into())),
            Ok(Some("pos".into()))
        );
        assert_eq!(normalize_search(Some("   ".into())), Ok(None));
        assert_eq!(
            normalize_search(Some("x".repeat(MAX_SEARCH_LEN + 1))),
            Err(ControlError::InvalidInput)
        );
    }

    #[test]
    fn pagina_conserva_limite_y_emite_cursor_del_ultimo_elemento() {
        let now = OffsetDateTime::now_utc();
        let mut items = vec![
            ControlOrganizationSummary {
                id: OrganizationId::new(),
                display_name: "A".into(),
                status: OrganizationStatus::Active,
                created_at: now,
            },
            ControlOrganizationSummary {
                id: OrganizationId::new(),
                display_name: "B".into(),
                status: OrganizationStatus::Active,
                created_at: now,
            },
            ControlOrganizationSummary {
                id: OrganizationId::new(),
                display_name: "C".into(),
                status: OrganizationStatus::Active,
                created_at: now,
            },
        ];
        let cursor = take_page_cursor(&mut items, 2, |item| ControlCursor {
            created_at: item.created_at,
            id: item.id.as_uuid(),
        });
        assert_eq!(items.len(), 2);
        assert_eq!(
            ControlCursor::decode(&cursor.unwrap()).unwrap().id,
            items[1].id.as_uuid()
        );
    }
}
