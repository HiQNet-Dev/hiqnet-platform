//! Mutaciones administrativas de OrganizationProduct (B4).
//!
//! `organization_product` es habilitación, no suscripción. No se borra
//! físicamente: `active|inactive` conserva la fila y `activated_at` es inmutable.

use async_trait::async_trait;
use platform_core::{
    ids::{OrganizationId, ProductId, StaffAccountId},
    staff::{Capability, StaffRole},
};
use std::str::FromStr;
use time::OffsetDateTime;

use crate::control::{AssociationStatus, ControlOrganizationProductView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OrganizationProductError {
    #[error("recurso no encontrado")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("solicitud inválida")]
    InvalidInput,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type OrganizationProductResult<T> = Result<T, OrganizationProductError>;

#[derive(Debug, Clone, PartialEq)]
pub struct OrganizationProductMutation {
    pub view: ControlOrganizationProductView,
    pub changed: bool,
}

#[async_trait]
pub trait OrganizationProductRepository: Send + Sync {
    async fn list(
        &self,
        organization_id: OrganizationId,
    ) -> OrganizationProductResult<Vec<ControlOrganizationProductView>>;
    async fn activate(
        &self,
        actor: StaffAccountId,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation>;
    async fn set_status(
        &self,
        actor: StaffAccountId,
        organization_id: OrganizationId,
        product_id: ProductId,
        status: AssociationStatus,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation>;
}

#[derive(Debug, Clone, Copy)]
pub struct OrganizationProductActor {
    account_id: StaffAccountId,
    capabilities: &'static [Capability],
}

impl OrganizationProductActor {
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

    fn require(&self, capability: Capability) -> OrganizationProductResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(OrganizationProductError::Forbidden)
        }
    }
}

pub struct OrganizationProductService<R: OrganizationProductRepository> {
    repository: R,
}

impl<R: OrganizationProductRepository> OrganizationProductService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list(
        &self,
        actor: &OrganizationProductActor,
        organization_id: OrganizationId,
    ) -> OrganizationProductResult<Vec<ControlOrganizationProductView>> {
        actor.require(Capability::OrganizationsRead)?;
        self.repository.list(organization_id).await
    }

    pub async fn activate(
        &self,
        actor: &OrganizationProductActor,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation> {
        actor.require(Capability::OrganizationsManage)?;
        self.repository
            .activate(actor.account_id, organization_id, product_id, now)
            .await
    }

    pub async fn set_status(
        &self,
        actor: &OrganizationProductActor,
        organization_id: OrganizationId,
        product_id: ProductId,
        status: AssociationStatus,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation> {
        actor.require(Capability::OrganizationsManage)?;
        self.repository
            .set_status(actor.account_id, organization_id, product_id, status, now)
            .await
    }
}

pub fn parse_association_status(value: &str) -> OrganizationProductResult<AssociationStatus> {
    AssociationStatus::from_str(value).map_err(|_| OrganizationProductError::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_manage_can_mutate_and_read_is_separate() {
        let reader = OrganizationProductActor::from_capabilities(
            StaffAccountId::new(),
            &[Capability::OrganizationsRead],
        );
        assert!(reader.require(Capability::OrganizationsRead).is_ok());
        assert_eq!(
            reader.require(Capability::OrganizationsManage),
            Err(OrganizationProductError::Forbidden)
        );
        let manager =
            OrganizationProductActor::new(StaffAccountId::new(), StaffRole::PlatformAdmin);
        assert!(manager.require(Capability::OrganizationsManage).is_ok());
    }

    #[test]
    fn association_status_is_closed() {
        assert_eq!(
            parse_association_status("active"),
            Ok(AssociationStatus::Active)
        );
        assert_eq!(
            parse_association_status("inactive"),
            Ok(AssociationStatus::Inactive)
        );
        assert_eq!(
            parse_association_status("deleted"),
            Err(OrganizationProductError::InvalidInput)
        );
    }
}
