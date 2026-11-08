//! Casos de uso comerciales internos; el actor deberá provenir de una sesión
//! verificada cuando P02.2 añada transporte HTTP.

use async_trait::async_trait;
use platform_core::{
    commercial::{
        CommercialAccount, CommercialOrganization, CommercialResult, OrganizationMembership,
        OrganizationRole,
    },
    ids::{CommercialAccountId, InstallationId, OrganizationId},
    installation::Installation,
};
use time::OffsetDateTime;

/// Vista mínima de una organización para el actor autenticado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationView {
    pub id: OrganizationId,
    pub display_name: String,
    pub role: OrganizationRole,
    pub status: &'static str,
}

#[async_trait]
pub trait CommercialRepository: Send + Sync {
    async fn create_account(&self, account: &CommercialAccount) -> CommercialResult<()>;
    async fn create_organization(
        &self,
        actor: CommercialAccountId,
        organization: &CommercialOrganization,
    ) -> CommercialResult<()>;
    async fn add_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()>;
    async fn change_role(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()>;
    async fn revoke_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<()>;
    async fn list_members(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<Vec<OrganizationMembership>>;
    async fn get_installation(
        &self,
        actor: CommercialAccountId,
        installation: InstallationId,
    ) -> CommercialResult<Installation>;
    /// Organizaciones activas donde el actor tiene membresía activa.
    async fn list_organizations(
        &self,
        actor: CommercialAccountId,
    ) -> CommercialResult<Vec<OrganizationView>>;
    /// Organización activa con membresía activa del actor; `NotFound` si no.
    async fn get_organization(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<OrganizationView>;
}

pub struct CommercialService<R: CommercialRepository> {
    repository: R,
}

impl<R: CommercialRepository> CommercialService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// No publica cuentas: P02.2 deberá autenticar al actor antes de invocar este caso.
    pub async fn create_account(
        &self,
        email: &str,
        name: &str,
        now: OffsetDateTime,
    ) -> CommercialResult<CommercialAccount> {
        let account = CommercialAccount::new(email, name, now)?;
        self.repository.create_account(&account).await?;
        Ok(account)
    }

    pub async fn create_organization(
        &self,
        actor: CommercialAccountId,
        name: &str,
        now: OffsetDateTime,
    ) -> CommercialResult<CommercialOrganization> {
        let organization = CommercialOrganization::new(name, now)?;
        self.repository
            .create_organization(actor, &organization)
            .await?;
        Ok(organization)
    }

    pub async fn add_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        self.repository
            .add_member(actor, organization, member, role, now)
            .await
    }

    pub async fn change_role(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        self.repository
            .change_role(actor, organization, member, role, now)
            .await
    }

    pub async fn revoke_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        self.repository
            .revoke_member(actor, organization, member, now)
            .await
    }

    pub async fn list_members(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<Vec<OrganizationMembership>> {
        self.repository.list_members(actor, organization).await
    }

    pub async fn get_installation(
        &self,
        actor: CommercialAccountId,
        installation: InstallationId,
    ) -> CommercialResult<Installation> {
        self.repository.get_installation(actor, installation).await
    }

    pub async fn list_organizations(
        &self,
        actor: CommercialAccountId,
    ) -> CommercialResult<Vec<OrganizationView>> {
        self.repository.list_organizations(actor).await
    }

    pub async fn get_organization(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<OrganizationView> {
        self.repository.get_organization(actor, organization).await
    }
}
