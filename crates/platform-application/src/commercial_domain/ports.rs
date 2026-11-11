//! Puerto de persistencia y actor autorizado del dominio comercial.

use super::*;
use async_trait::async_trait;

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
    pub(super) account_id: StaffAccountId,
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

    /// Cuenta de personal que ejecuta la operación (para auditoría).
    pub fn staff_account_id(&self) -> StaffAccountId {
        self.account_id
    }

    /// Consulta de capacidad para servicios fuera del módulo comercial.
    pub fn has_capability(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub(super) fn require(&self, capability: Capability) -> CommercialResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(CommercialError::Forbidden)
        }
    }
}
