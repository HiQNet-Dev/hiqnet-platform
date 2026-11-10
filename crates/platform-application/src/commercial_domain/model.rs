//! Modelos, estados, errores y utilidades de parseo del dominio comercial.
//!
//! Sin dependencias de HTTP ni de persistencia: sólo tipos e invariantes.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommercialError {
    #[error("recurso no encontrado")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("solicitud inválida")]
    InvalidInput,
    #[error("conflicto de estado")]
    Conflict,
    #[error("conflicto de idempotencia")]
    IdempotencyConflict,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type CommercialResult<T> = Result<T, CommercialError>;

pub const DEFAULT_PAGE_LIMIT: u32 = 50;

pub const MAX_PAGE_LIMIT: u32 = 100;

pub const MAX_CURSOR_LEN: usize = 256;

pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Draft,
    Active,
    Maintenance,
    Deprecated,
    Retired,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Maintenance => "maintenance",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }

    pub(super) fn can_transition_to(self, target: Self) -> bool {
        match (self, target) {
            (Self::Draft, Self::Active | Self::Retired) => true,
            (Self::Active, Self::Maintenance | Self::Deprecated | Self::Retired) => true,
            (Self::Maintenance, Self::Active | Self::Deprecated | Self::Retired) => true,
            (Self::Deprecated, Self::Retired) => true,
            (Self::Retired, _) => false,
            (current, target) => current == target,
        }
    }
}

impl FromStr for PlanStatus {
    type Err = CommercialError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "active" => Ok(Self::Active),
            "maintenance" => Ok(Self::Maintenance),
            "deprecated" => Ok(Self::Deprecated),
            "retired" => Ok(Self::Retired),
            _ => Err(CommercialError::InvalidInput),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Active,
    Suspended,
    Cancelled,
    Expired,
}

impl SubscriptionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Expired)
    }
}

impl FromStr for SubscriptionStatus {
    type Err = CommercialError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "cancelled" => Ok(Self::Cancelled),
            "expired" => Ok(Self::Expired),
            _ => Err(CommercialError::InvalidInput),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub id: Uuid,
    pub product_id: ProductId,
    pub product_key: String,
    pub product_name: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: PlanStatus,
    pub is_trial: bool,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
    pub subscription_count: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRef {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub is_trial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub id: Uuid,
    pub organization_id: OrganizationId,
    pub product_id: ProductId,
    pub plan_id: Uuid,
    pub plan: PlanRef,
    pub status: SubscriptionStatus,
    pub is_trial: bool,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub suspended_at: Option<OffsetDateTime>,
    pub cancelled_at: Option<OffsetDateTime>,
    pub renewed_from_subscription_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionEvent {
    pub id: Uuid,
    pub event: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub occurred_at: OffsetDateTime,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionDetail {
    pub subscription: Subscription,
    pub events: Vec<SubscriptionEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entitlements {
    pub effective: bool,
    pub status: Option<SubscriptionStatus>,
    pub reason: String,
    pub plan: Option<PlanRef>,
    pub subscription_id: Option<Uuid>,
    pub is_trial: bool,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
    pub starts_at: Option<OffsetDateTime>,
    pub ends_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanFilters {
    pub product_id: Option<ProductId>,
    pub status: Option<PlanStatus>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubscriptionFilters {
    pub organization_id: Option<OrganizationId>,
    pub product_id: Option<ProductId>,
    pub status: Option<SubscriptionStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlan {
    pub product_id: ProductId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: PlanStatus,
    pub is_trial: bool,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<PlanStatus>,
    pub features: Option<BTreeMap<String, bool>>,
    pub limits: Option<BTreeMap<String, i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSubscription {
    pub organization_id: OrganizationId,
    pub product_id: ProductId,
    pub plan_id: Uuid,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSubscriptionCommand {
    pub organization_id: OrganizationId,
    pub product_id: ProductId,
    pub plan_id: Uuid,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub idempotency_key_hash: Vec<u8>,
    pub request_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewSubscription {
    pub plan_id: Option<Uuid>,
    pub ends_at: OffsetDateTime,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewSubscriptionCommand {
    pub plan_id: Option<Uuid>,
    pub ends_at: OffsetDateTime,
    pub idempotency_key_hash: Vec<u8>,
    pub request_hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionTransition {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionAction {
    Suspend,
    Reactivate,
    Cancel,
}

impl SubscriptionAction {
    pub fn audit_action(self) -> &'static str {
        match self {
            Self::Suspend => "subscription_suspended",
            Self::Reactivate => "subscription_reactivated",
            Self::Cancel => "subscription_cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorKey {
    pub timestamp: OffsetDateTime,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalProductRef {
    pub id: ProductId,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalSubscription {
    pub id: Uuid,
    pub status: SubscriptionStatus,
    pub plan: PlanRef,
    pub starts_at: OffsetDateTime,
    pub ends_at: OffsetDateTime,
    pub is_trial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalProduct {
    pub product: PortalProductRef,
    pub organization_product_status: String,
    pub subscription: Option<PortalSubscription>,
    pub entitlements: Entitlements,
}

pub fn parse_plan_status(value: &str) -> CommercialResult<PlanStatus> {
    PlanStatus::from_str(value)
}

pub fn parse_subscription_status(value: &str) -> CommercialResult<SubscriptionStatus> {
    SubscriptionStatus::from_str(value)
}

pub fn sanitize_metadata(value: Value) -> Value {
    match value {
        Value::Object(_) => value,

        _ => serde_json::json!({}),
    }
}
