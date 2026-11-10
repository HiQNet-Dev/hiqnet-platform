//! Commercial Core mínimo de Beta (ADR-0011).
//!
//! Plan es catálogo por producto; Subscription es el contrato con historial;
//! Entitlement es un read model derivado, nunca `organization_product.entitlements`.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::{
    ids::{OrganizationId, ProductId, StaffAccountId},
    staff::{Capability, StaffRole},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

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

    fn can_transition_to(self, target: Self) -> bool {
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

#[async_trait]
pub trait CommercialRepository: Send + Sync {
    async fn list_plans(
        &self,
        filters: &PlanFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> CommercialResult<Vec<Plan>>;
    async fn get_plan(&self, id: Uuid) -> CommercialResult<Option<Plan>>;
    async fn create_plan(
        &self,
        actor: StaffAccountId,
        plan: NewPlan,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan>;
    async fn update_plan(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        patch: PlanPatch,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan>;
    async fn list_subscriptions(
        &self,
        filters: &SubscriptionFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> CommercialResult<Vec<Subscription>>;
    async fn get_subscription(&self, id: Uuid) -> CommercialResult<Option<SubscriptionDetail>>;
    async fn create_subscription(
        &self,
        actor: StaffAccountId,
        command: CreateSubscriptionCommand,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription>;
    async fn transition_subscription(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        action: SubscriptionAction,
        reason: Option<String>,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription>;
    async fn renew_subscription(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        command: RenewSubscriptionCommand,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription>;
    async fn entitlements(
        &self,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<Entitlements>>;
    async fn portal_products(
        &self,
        organization_id: OrganizationId,
        account_id: platform_core::ids::CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<Vec<PortalProduct>>>;
    async fn portal_product(
        &self,
        organization_id: OrganizationId,
        product_id: ProductId,
        account_id: platform_core::ids::CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<PortalProduct>>;
}

#[derive(Debug, Clone, Copy)]
pub struct CommercialActor {
    account_id: StaffAccountId,
    capabilities: &'static [Capability],
}

impl CommercialActor {
    pub fn new(account_id: StaffAccountId, role: StaffRole) -> Self {
        Self {
            account_id,
            capabilities: role.capabilities(),
        }
    }

    pub fn from_capabilities(
        account_id: StaffAccountId,
        capabilities: &'static [Capability],
    ) -> Self {
        Self {
            account_id,
            capabilities,
        }
    }

    fn require(&self, capability: Capability) -> CommercialResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(CommercialError::Forbidden)
        }
    }
}

pub struct CommercialService<R: CommercialRepository> {
    repository: R,
}

impl<R: CommercialRepository> CommercialService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list_plans(
        &self,
        actor: &CommercialActor,
        filters: PlanFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> CommercialResult<Page<Plan>> {
        actor.require(Capability::ProductsRead)?;
        let (limit, after) = page_params(limit, cursor.as_deref(), "plan")?;
        let mut items = self
            .repository
            .list_plans(&filters, limit + 1, after)
            .await?;
        let next_cursor =
            take_page_cursor(&mut items, limit, "plan", |item| (item.created_at, item.id));
        Ok(Page { items, next_cursor })
    }

    pub async fn get_plan(&self, actor: &CommercialActor, id: Uuid) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsRead)?;
        self.repository
            .get_plan(id)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn create_plan(
        &self,
        actor: &CommercialActor,
        plan: NewPlan,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsManage)?;
        validate_plan(&plan)?;
        self.repository
            .create_plan(actor.account_id, plan, now)
            .await
    }

    pub async fn update_plan(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        patch: PlanPatch,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsManage)?;
        if let Some(name) = patch.name.as_deref() {
            validate_name(name)?;
        }
        if let Some(features) = &patch.features {
            validate_features(features)?;
        }
        if let Some(limits) = &patch.limits {
            validate_limits(limits)?;
        }
        let current = self
            .repository
            .get_plan(id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if let Some(status) = patch.status {
            if !current.status.can_transition_to(status) {
                return Err(CommercialError::Conflict);
            }
        }
        if current.subscription_count > 0 && (patch.features.is_some() || patch.limits.is_some()) {
            return Err(CommercialError::Conflict);
        }
        self.repository
            .update_plan(actor.account_id, id, patch, now)
            .await
    }

    pub async fn list_subscriptions(
        &self,
        actor: &CommercialActor,
        filters: SubscriptionFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> CommercialResult<Page<Subscription>> {
        actor.require(Capability::SubscriptionsRead)?;
        let (limit, after) = page_params(limit, cursor.as_deref(), "subscription")?;
        let mut items = self
            .repository
            .list_subscriptions(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, "subscription", |item| {
            (item.created_at, item.id)
        });
        Ok(Page { items, next_cursor })
    }

    pub async fn get_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
    ) -> CommercialResult<SubscriptionDetail> {
        actor.require(Capability::SubscriptionsRead)?;
        self.repository
            .get_subscription(id)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn create_subscription(
        &self,
        actor: &CommercialActor,
        request: CreateSubscription,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.ends_at <= request.starts_at {
            return Err(CommercialError::InvalidInput);
        }
        let command = CreateSubscriptionCommand {
            organization_id: request.organization_id,
            product_id: request.product_id,
            plan_id: request.plan_id,
            starts_at: request.starts_at,
            ends_at: request.ends_at,
            idempotency_key_hash: hash_secret(request.idempotency_key.as_bytes()).to_vec(),
            request_hash: create_request_hash(&request),
        };
        self.repository
            .create_subscription(actor.account_id, command, now)
            .await
    }

    pub async fn transition_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        action: SubscriptionAction,
        transition: SubscriptionTransition,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        if let Some(reason) = transition.reason.as_deref() {
            if reason.trim().is_empty() || reason.chars().count() > 500 {
                return Err(CommercialError::InvalidInput);
            }
        }
        self.repository
            .transition_subscription(actor.account_id, id, action, transition.reason, now)
            .await
    }

    pub async fn renew_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        request: RenewSubscription,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.ends_at <= now {
            return Err(CommercialError::InvalidInput);
        }
        let command = RenewSubscriptionCommand {
            plan_id: request.plan_id,
            ends_at: request.ends_at,
            idempotency_key_hash: hash_secret(request.idempotency_key.as_bytes()).to_vec(),
            request_hash: renew_request_hash(&request),
        };
        self.repository
            .renew_subscription(actor.account_id, id, command, now)
            .await
    }

    pub async fn entitlements(
        &self,
        actor: &CommercialActor,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> CommercialResult<Entitlements> {
        actor.require(Capability::SubscriptionsRead)?;
        self.repository
            .entitlements(organization_id, product_id, now)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn portal_products(
        &self,
        organization_id: OrganizationId,
        account_id: platform_core::ids::CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<Vec<PortalProduct>> {
        self.repository
            .portal_products(organization_id, account_id, now)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn portal_product(
        &self,
        organization_id: OrganizationId,
        product_id: ProductId,
        account_id: platform_core::ids::CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<PortalProduct> {
        self.repository
            .portal_product(organization_id, product_id, account_id, now)
            .await?
            .ok_or(CommercialError::NotFound)
    }
}

fn validate_plan(plan: &NewPlan) -> CommercialResult<()> {
    validate_key(&plan.key)?;
    validate_name(&plan.name)?;
    if plan.description.chars().count() > 2000 {
        return Err(CommercialError::InvalidInput);
    }
    validate_features(&plan.features)?;
    validate_limits(&plan.limits)?;
    Ok(())
}

fn validate_name(value: &str) -> CommercialResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

fn validate_key(value: &str) -> CommercialResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

fn validate_features(features: &BTreeMap<String, bool>) -> CommercialResult<()> {
    for key in features.keys() {
        validate_key(key)?;
    }
    Ok(())
}

fn validate_limits(limits: &BTreeMap<String, i64>) -> CommercialResult<()> {
    for (key, value) in limits {
        validate_key(key)?;
        if *value < 0 {
            return Err(CommercialError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> CommercialResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_IDEMPOTENCY_KEY_LEN {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

fn hash_secret(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn create_request_hash(request: &CreateSubscription) -> Vec<u8> {
    let canonical = format!(
        "create:{}:{}:{}:{}:{}",
        request.organization_id.as_uuid(),
        request.product_id.as_uuid(),
        request.plan_id,
        request.starts_at.unix_timestamp_nanos(),
        request.ends_at.unix_timestamp_nanos()
    );
    hash_secret(canonical.as_bytes()).to_vec()
}

fn renew_request_hash(request: &RenewSubscription) -> Vec<u8> {
    let canonical = format!(
        "renew:{}:{}",
        request
            .plan_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".into()),
        request.ends_at.unix_timestamp_nanos()
    );
    hash_secret(canonical.as_bytes()).to_vec()
}

fn page_params(
    limit: Option<u32>,
    cursor: Option<&str>,
    dataset: &str,
) -> CommercialResult<(u32, Option<CursorKey>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(CommercialError::InvalidInput);
    }
    let after = cursor
        .map(|value| decode_cursor(value, dataset))
        .transpose()?;
    Ok((limit, after))
}

fn take_page_cursor<T>(
    items: &mut Vec<T>,
    limit: u32,
    dataset: &str,
    key: impl Fn(&T) -> (OffsetDateTime, Uuid),
) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(|item| {
            let (timestamp, id) = key(item);
            encode_cursor(dataset, timestamp, id)
        })
    } else {
        None
    }
}

fn encode_cursor(dataset: &str, timestamp: OffsetDateTime, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!(
        "{}:{}:{}",
        dataset,
        timestamp.unix_timestamp_nanos(),
        id
    ))
}

fn decode_cursor(value: &str, dataset: &str) -> CommercialResult<CursorKey> {
    if value.is_empty() || value.len() > MAX_CURSOR_LEN {
        return Err(CommercialError::InvalidInput);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CommercialError::InvalidInput)?;
    let text = String::from_utf8(decoded).map_err(|_| CommercialError::InvalidInput)?;
    let mut parts = text.splitn(3, ':');
    let actual_dataset = parts.next().ok_or(CommercialError::InvalidInput)?;
    let timestamp = parts.next().ok_or(CommercialError::InvalidInput)?;
    let id = parts.next().ok_or(CommercialError::InvalidInput)?;
    if actual_dataset != dataset {
        return Err(CommercialError::InvalidInput);
    }
    let timestamp = OffsetDateTime::from_unix_timestamp_nanos(
        timestamp
            .parse()
            .map_err(|_| CommercialError::InvalidInput)?,
    )
    .map_err(|_| CommercialError::InvalidInput)?;
    Ok(CursorKey {
        timestamp,
        id: Uuid::parse_str(id).map_err(|_| CommercialError::InvalidInput)?,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: StaffRole) -> CommercialActor {
        CommercialActor::new(StaffAccountId::new(), role)
    }

    #[test]
    fn capabilities_follow_beta_matrix() {
        for role in [
            StaffRole::SuperAdmin,
            StaffRole::PlatformAdmin,
            StaffRole::Operator,
            StaffRole::Support,
            StaffRole::SecurityAuditor,
            StaffRole::ReadOnly,
        ] {
            assert!(actor(role).require(Capability::ProductsRead).is_ok());
            assert!(actor(role).require(Capability::SubscriptionsRead).is_ok());
        }
        for role in [
            StaffRole::Operator,
            StaffRole::Support,
            StaffRole::SecurityAuditor,
            StaffRole::ReadOnly,
        ] {
            assert_eq!(
                actor(role).require(Capability::ProductsManage),
                Err(CommercialError::Forbidden)
            );
            assert_eq!(
                actor(role).require(Capability::SubscriptionsManage),
                Err(CommercialError::Forbidden)
            );
        }
        assert!(
            actor(StaffRole::PlatformAdmin)
                .require(Capability::SubscriptionsManage)
                .is_ok()
        );
    }

    #[test]
    fn plan_status_transitions_are_closed() {
        assert!(PlanStatus::Draft.can_transition_to(PlanStatus::Active));
        assert!(!PlanStatus::Retired.can_transition_to(PlanStatus::Active));
        assert_eq!(parse_plan_status("active"), Ok(PlanStatus::Active));
        assert_eq!(
            parse_plan_status("deleted"),
            Err(CommercialError::InvalidInput)
        );
    }

    #[test]
    fn cursors_are_dataset_bound() {
        let timestamp = OffsetDateTime::now_utc();
        let id = Uuid::new_v4();
        let cursor = encode_cursor("plan", timestamp, id);
        assert_eq!(
            decode_cursor(&cursor, "plan").unwrap(),
            CursorKey { timestamp, id }
        );
        assert_eq!(
            decode_cursor(&cursor, "subscription"),
            Err(CommercialError::InvalidInput)
        );
        assert_eq!(
            decode_cursor("garbage", "plan"),
            Err(CommercialError::InvalidInput)
        );
    }

    #[test]
    fn idempotency_and_validation_are_strict() {
        assert_eq!(
            validate_idempotency_key("  "),
            Err(CommercialError::InvalidInput)
        );
        assert!(validate_idempotency_key("key-1").is_ok());
        assert_eq!(validate_key("bad key"), Err(CommercialError::InvalidInput));
        assert!(validate_key("pos-local").is_ok());
        assert_eq!(
            validate_limits(&BTreeMap::from([("seats".into(), -1)])),
            Err(CommercialError::InvalidInput)
        );
    }
}
