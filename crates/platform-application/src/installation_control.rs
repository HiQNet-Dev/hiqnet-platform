//! Control administrativo de instalaciones POS.
//!
//! Read-side staff y mutaciones semánticas. No reutiliza autoridad comercial;
//! comparte almacenamiento con Enrollment/Licensing.

use async_trait::async_trait;
use platform_core::{
    ids::{
        CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId, ProductId,
        StaffAccountId,
    },
    staff::{Capability, StaffRole},
};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InstallationControlError {
    #[error("instalación no encontrada")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("solicitud inválida")]
    InvalidInput,
    #[error("transición de estado no permitida")]
    TransitionConflict,
    #[error("conflicto de rehost")]
    RehostConflict,
    #[error("autorización de rehost expirada")]
    RehostExpired,
    #[error("producto no disponible para la instalación")]
    ProductUnavailable,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type InstallationControlResult<T> = Result<T, InstallationControlError>;

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 100;
pub const MAX_SEARCH_LEN: usize = 200;
pub const MAX_CURSOR_LEN: usize = 128;
pub const MAX_REASON_LEN: usize = 500;

macro_rules! control_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl FromStr for $name {
            type Err = InstallationControlError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value { $($text => Ok(Self::$variant),)+ _ => Err(InstallationControlError::Storage) }
            }
        }
    };
}

control_enum!(AdministrativeStatus {
    Active => "active",
    Suspended => "suspended",
    Unregistered => "unregistered",
});

control_enum!(IdentityStatus {
    Unenrolled => "unenrolled",
    Active => "active",
    Retired => "retired",
});

control_enum!(RehostStatus {
    Pending => "pending",
    Consumed => "consumed",
    Expired => "expired",
    Cancelled => "cancelled",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationSummary {
    pub id: InstallationId,
    pub device_label: String,
    pub organization_id: OrganizationId,
    pub organization_display_name: String,
    pub organization_status: String,
    pub product_id: Option<ProductId>,
    pub product_key: Option<String>,
    pub product_name: Option<String>,
    pub administrative_status: AdministrativeStatus,
    pub identity_status: IdentityStatus,
    pub registered_at: OffsetDateTime,
    pub enrolled_at: Option<OffsetDateTime>,
    pub last_known_activity_at: Option<OffsetDateTime>,
    pub identity_generation: Option<u32>,
    pub binding_active: bool,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentBindingView {
    pub bound_at: OffsetDateTime,
    pub approved_by_account_id: CommercialAccountId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehostAuthorizationView {
    pub id: Uuid,
    pub enrollment_request_id: EnrollmentRequestId,
    pub status: RehostStatus,
    pub reason: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationDetail {
    pub summary: InstallationSummary,
    pub current_identity_created_at: Option<OffsetDateTime>,
    pub current_binding: Option<CurrentBindingView>,
    pub rehost: Option<RehostAuthorizationView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlCursor {
    pub registered_at: OffsetDateTime,
    pub id: Uuid,
}

impl ControlCursor {
    pub fn encode(self) -> String {
        base64_url(format!(
            "{}:{}",
            self.registered_at.unix_timestamp_nanos(),
            self.id
        ))
    }

    pub fn decode(value: &str) -> InstallationControlResult<Self> {
        if value.is_empty() || value.len() > MAX_CURSOR_LEN {
            return Err(InstallationControlError::InvalidInput);
        }
        let decoded = base64_url_decode(value)?;
        let text =
            String::from_utf8(decoded).map_err(|_| InstallationControlError::InvalidInput)?;
        let (timestamp, id) = text
            .split_once(':')
            .ok_or(InstallationControlError::InvalidInput)?;
        let nanos: i128 = timestamp
            .parse()
            .map_err(|_| InstallationControlError::InvalidInput)?;
        let registered_at = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| InstallationControlError::InvalidInput)?;
        let id = Uuid::parse_str(id).map_err(|_| InstallationControlError::InvalidInput)?;
        Ok(Self { registered_at, id })
    }
}

fn base64_url(value: String) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(value)
}

fn base64_url_decode(value: &str) -> InstallationControlResult<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| InstallationControlError::InvalidInput)
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

    pub fn require(&self, capability: Capability) -> InstallationControlResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(InstallationControlError::Forbidden)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstallationFilters {
    pub status: Option<AdministrativeStatus>,
    pub identity_status: Option<IdentityStatus>,
    pub organization_id: Option<OrganizationId>,
    pub product_id: Option<ProductId>,
    pub search: Option<String>,
}

#[async_trait]
pub trait InstallationControlRepository: Send + Sync {
    async fn list_installations(
        &self,
        filters: &InstallationFilters,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> InstallationControlResult<Vec<InstallationSummary>>;
    async fn get_installation(
        &self,
        id: InstallationId,
    ) -> InstallationControlResult<Option<InstallationDetail>>;
    async fn set_administrative_status(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        target: AdministrativeStatus,
        now: OffsetDateTime,
    ) -> InstallationControlResult<InstallationDetail>;
    async fn authorize_rehost(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        enrollment_request_id: EnrollmentRequestId,
        reason: &str,
        now: OffsetDateTime,
    ) -> InstallationControlResult<RehostAuthorizationView>;
}

pub struct InstallationControlService<R: InstallationControlRepository> {
    repository: R,
}

impl<R: InstallationControlRepository> InstallationControlService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list(
        &self,
        actor: &ControlActor,
        filters: InstallationFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> InstallationControlResult<Page<InstallationSummary>> {
        actor.require(Capability::InstallationsRead)?;
        let (limit, after) = page_params(limit, cursor)?;
        let filters = validate_filters(filters)?;
        let mut items = self
            .repository
            .list_installations(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit);
        Ok(Page { items, next_cursor })
    }

    pub async fn get(
        &self,
        actor: &ControlActor,
        id: InstallationId,
    ) -> InstallationControlResult<InstallationDetail> {
        actor.require(Capability::InstallationsRead)?;
        self.repository
            .get_installation(id)
            .await?
            .ok_or(InstallationControlError::NotFound)
    }

    pub async fn suspend(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        now: OffsetDateTime,
    ) -> InstallationControlResult<InstallationDetail> {
        actor.require(Capability::InstallationsManage)?;
        self.repository
            .set_administrative_status(actor, id, AdministrativeStatus::Suspended, now)
            .await
    }

    pub async fn reactivate(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        now: OffsetDateTime,
    ) -> InstallationControlResult<InstallationDetail> {
        actor.require(Capability::InstallationsManage)?;
        self.repository
            .set_administrative_status(actor, id, AdministrativeStatus::Active, now)
            .await
    }

    pub async fn authorize_rehost(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        enrollment_request_id: EnrollmentRequestId,
        reason: String,
        now: OffsetDateTime,
    ) -> InstallationControlResult<RehostAuthorizationView> {
        actor.require(Capability::InstallationsRehost)?;
        let reason = reason.trim();
        if reason.is_empty() || reason.chars().count() > MAX_REASON_LEN {
            return Err(InstallationControlError::InvalidInput);
        }
        self.repository
            .authorize_rehost(actor, id, enrollment_request_id, reason, now)
            .await
    }
}

fn page_params(
    limit: Option<u32>,
    cursor: Option<String>,
) -> InstallationControlResult<(u32, Option<ControlCursor>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(InstallationControlError::InvalidInput);
    }
    let after = cursor
        .map(|value| ControlCursor::decode(&value))
        .transpose()?;
    Ok((limit, after))
}

fn validate_filters(
    mut filters: InstallationFilters,
) -> InstallationControlResult<InstallationFilters> {
    filters.search = match filters.search {
        None => None,
        Some(search) => {
            let trimmed = search.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.chars().count() > MAX_SEARCH_LEN {
                return Err(InstallationControlError::InvalidInput);
            } else {
                Some(trimmed.to_owned())
            }
        }
    };
    Ok(filters)
}

fn take_page_cursor(items: &mut Vec<InstallationSummary>, limit: u32) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(|item| {
            ControlCursor {
                registered_at: item.registered_at,
                id: item.id.as_uuid(),
            }
            .encode()
        })
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
    fn rehost_es_independiente_de_manage() {
        assert!(
            actor(StaffRole::PlatformAdmin)
                .require(Capability::InstallationsRehost)
                .is_ok()
        );
        assert_eq!(
            actor(StaffRole::Operator).require(Capability::InstallationsRehost),
            Err(InstallationControlError::Forbidden)
        );
        assert!(
            actor(StaffRole::Operator)
                .require(Capability::InstallationsManage)
                .is_ok()
        );
    }

    #[test]
    fn matriz_rbac_c5_por_rol() {
        // (rol, read, manage, rehost)
        let cases = [
            (StaffRole::SuperAdmin, true, true, true),
            (StaffRole::PlatformAdmin, true, true, true),
            (StaffRole::Operator, true, true, false),
            (StaffRole::Support, true, false, false),
            (StaffRole::SecurityAuditor, true, false, false),
            (StaffRole::ReadOnly, true, false, false),
        ];
        for (role, read, manage, rehost) in cases {
            let actor = actor(role);
            assert_eq!(
                actor.require(Capability::InstallationsRead).is_ok(),
                read,
                "{role}: installations.read"
            );
            assert_eq!(
                actor.require(Capability::InstallationsManage).is_ok(),
                manage,
                "{role}: installations.manage"
            );
            assert_eq!(
                actor.require(Capability::InstallationsRehost).is_ok(),
                rehost,
                "{role}: installations.rehost"
            );
        }
    }

    #[test]
    fn limites_y_busqueda_son_estrictos() {
        assert!(page_params(Some(0), None).is_err());
        assert!(page_params(Some(MAX_PAGE_LIMIT + 1), None).is_err());
        assert!(
            validate_filters(InstallationFilters {
                search: Some("x".repeat(MAX_SEARCH_LEN + 1)),
                ..Default::default()
            })
            .is_err()
        );
        assert_eq!(
            validate_filters(InstallationFilters {
                search: Some("  caja  ".into()),
                ..Default::default()
            })
            .unwrap()
            .search
            .as_deref(),
            Some("caja")
        );
    }

    #[test]
    fn cursor_sobrevive_ida_y_vuelta() {
        let cursor = ControlCursor {
            registered_at: OffsetDateTime::now_utc(),
            id: Uuid::new_v4(),
        };
        assert_eq!(ControlCursor::decode(&cursor.encode()).unwrap(), cursor);
        assert!(ControlCursor::decode("bogus").is_err());
    }
}
