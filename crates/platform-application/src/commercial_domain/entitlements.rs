//! Resolución de derechos efectivos y consulta comercial del Portal.

use super::*;

impl<R: CommercialRepository> CommercialService<R> {
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
