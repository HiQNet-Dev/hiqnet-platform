use async_trait::async_trait;
use platform_core::ids::{InstallationId, OrganizationId};
use platform_core::installation::Installation;
use platform_core::license::IssuedLicense;
use platform_core::plan::PlanDefinition;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[async_trait]
pub trait PlatformRepository: Send + Sync {
    async fn list_plans(&self) -> AppResult<Vec<PlanDefinition>>;
    async fn register_installation(&self, installation: &Installation) -> AppResult<Installation>;
    async fn get_installation(&self, id: InstallationId) -> AppResult<Option<Installation>>;
    async fn start_trial(&self, id: InstallationId, now: OffsetDateTime)
    -> AppResult<Installation>;
    async fn record_license(&self, license: &IssuedLicense) -> AppResult<()>;
    async fn list_licenses(&self, installation_id: InstallationId)
    -> AppResult<Vec<IssuedLicense>>;
}

#[async_trait]
pub trait LicenseSigner: Send + Sync {
    fn sign(&self, grant_json: &str) -> AppResult<String>;
    fn public_key_b64(&self) -> String;
}

pub type SharedRepository = std::sync::Arc<dyn PlatformRepository>;
pub type SharedSigner = std::sync::Arc<dyn LicenseSigner>;

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterInstallationRequest {
    pub installation_id: Option<Uuid>,
    pub organization_id: Uuid,
    pub device_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallationView {
    pub id: InstallationId,
    pub organization_id: OrganizationId,
    pub device_label: String,
    pub status: &'static str,
    pub registered_at: OffsetDateTime,
    pub trial_started_at: Option<OffsetDateTime>,
    pub trial_ends_at: Option<OffsetDateTime>,
}

impl From<&Installation> for InstallationView {
    fn from(value: &Installation) -> Self {
        Self {
            id: value.id,
            organization_id: value.organization_id,
            device_label: value.device_label.clone(),
            status: value.status.as_str(),
            registered_at: value.registered_at,
            trial_started_at: value.trial_started_at,
            trial_ends_at: value.trial_ends_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueLicenseRequest {
    pub plan: String,
    pub valid_until: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub plan: String,
    pub display_name: String,
    pub features: Vec<String>,
}

impl From<&PlanDefinition> for PlanView {
    fn from(value: &PlanDefinition) -> Self {
        Self {
            plan: value.plan.as_str().to_owned(),
            display_name: value.display_name.to_owned(),
            features: value
                .features
                .iter()
                .map(|feature| feature.as_str().to_owned())
                .collect(),
        }
    }
}

pub type AppResult<T> = crate::error::AppResult<T>;
