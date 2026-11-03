use crate::error::{AppError, AppResult};
use crate::ports::{
    InstallationView, IssueLicenseRequest, PlanView, RegisterInstallationRequest, SharedRepository,
    SharedSigner,
};
use platform_core::ids::{GrantId, InstallationId, OrganizationId};
use platform_core::installation::Installation;
use platform_core::license::{IssuedLicense, LICENSE_DOC_VERSION, LicenseGrant};
use platform_core::plan::PlanDefinition;
use serde::Serialize;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub struct IssuedLicenseView {
    pub grant_id: GrantId,
    pub installation_id: InstallationId,
    pub plan: String,
    pub valid_until: OffsetDateTime,
    pub grant_json: String,
    pub signature_b64: String,
    pub public_key_b64: String,
}

pub struct PlatformService {
    repository: SharedRepository,
    signer: Option<SharedSigner>,
}

impl PlatformService {
    pub fn new(repository: SharedRepository, signer: Option<SharedSigner>) -> Self {
        Self { repository, signer }
    }

    pub async fn list_plans(&self) -> AppResult<Vec<PlanView>> {
        let plans = self.repository.list_plans().await?;
        Ok(plans.iter().map(PlanView::from).collect())
    }

    pub async fn register_installation(
        &self,
        request: RegisterInstallationRequest,
        now: OffsetDateTime,
    ) -> AppResult<InstallationView> {
        let installation_id = request
            .installation_id
            .map(InstallationId::from_uuid)
            .unwrap_or_default();
        let organization_id = OrganizationId::from_uuid(request.organization_id);
        let installation =
            Installation::new(installation_id, organization_id, request.device_label, now)?;
        let stored = self.repository.register_installation(&installation).await?;
        Ok(InstallationView::from(&stored))
    }

    pub async fn get_installation(&self, id: InstallationId) -> AppResult<InstallationView> {
        let installation = self
            .repository
            .get_installation(id)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(InstallationView::from(&installation))
    }

    pub async fn start_trial(
        &self,
        id: InstallationId,
        now: OffsetDateTime,
    ) -> AppResult<InstallationView> {
        let installation = self.repository.start_trial(id, now).await?;
        Ok(InstallationView::from(&installation))
    }

    pub async fn issue_license(
        &self,
        id: InstallationId,
        request: IssueLicenseRequest,
        now: OffsetDateTime,
    ) -> AppResult<IssuedLicenseView> {
        let signer = self.signer.as_ref().ok_or(AppError::LicensingDisabled)?;
        let installation = self
            .repository
            .get_installation(id)
            .await?
            .ok_or(AppError::NotFound)?;
        let plan = platform_core::plan::PlanName::parse(&request.plan)?;
        let plan_str = plan.as_str().to_owned();
        let features: Vec<String> = PlanDefinition::catalog()
            .into_iter()
            .find(|entry| entry.plan == plan)
            .map(|entry| {
                entry
                    .features
                    .iter()
                    .map(|feature| feature.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default();

        IssuedLicense::validate_validity(now, request.valid_until)?;

        let grant = LicenseGrant {
            version: LICENSE_DOC_VERSION,
            grant_id: GrantId::new(),
            installation_id: installation.id,
            organization_id: installation.organization_id,
            plan: plan_str.clone(),
            features: features.clone(),
            issued_at: now,
            valid_until: request.valid_until,
        };
        let grant_json = String::from_utf8(
            grant
                .to_canonical_json()
                .map_err(|_| AppError::LicensingDisabled)?,
        )
        .map_err(|_| AppError::LicensingDisabled)?;
        let signature = signer.sign(&grant_json)?;
        let public_key_b64 = signer.public_key_b64();

        let issued = IssuedLicense {
            grant_id: grant.grant_id,
            installation_id: installation.id,
            organization_id: installation.organization_id,
            plan: plan_str,
            features_json: serde_json::to_string(&features).unwrap_or_else(|_| "[]".into()),
            issued_at: now,
            valid_until: request.valid_until,
            grant_json: grant_json.clone(),
            signature_b64: signature.clone(),
        };
        self.repository.record_license(&issued).await?;

        Ok(IssuedLicenseView {
            grant_id: issued.grant_id,
            installation_id: issued.installation_id,
            plan: issued.plan,
            valid_until: issued.valid_until,
            grant_json,
            signature_b64: signature,
            public_key_b64,
        })
    }

    pub async fn list_licenses(&self, id: InstallationId) -> AppResult<Vec<IssuedLicense>> {
        self.repository.list_licenses(id).await
    }
}
