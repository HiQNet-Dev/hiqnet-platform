use crate::commercial_domain::CommercialActor;
use crate::error::{AppError, AppResult};
use crate::license_issuance::EntitledPlan;
use crate::ports::{
    InstallationView, IssueLicenseRequest, PlanView, RegisterInstallationRequest, SharedRepository,
    SharedSigner,
};
use platform_core::ids::{GrantId, InstallationId, OrganizationId};
use platform_core::installation::Installation;
use platform_core::license::{IssuedLicense, LICENSE_DOC_VERSION, LicenseGrant};
use platform_core::plan::PlanDefinition;
use platform_core::staff::Capability;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct IssuedLicenseView {
    pub grant_id: GrantId,
    pub installation_id: InstallationId,
    pub plan: String,
    pub valid_until: OffsetDateTime,
    pub grant_json: String,
    pub signature_b64: String,
    pub public_key_b64: String,
    /// true cuando la respuesta es una recuperación del grant ya emitido
    /// (mismo documento), no una emisión nueva.
    pub recovered: bool,
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
            recovered: false,
        })
    }

    pub async fn list_licenses(&self, id: InstallationId) -> AppResult<Vec<IssuedLicense>> {
        self.repository.list_licenses(id).await
    }

    /// Audita un rechazo de emisión. Sólo se invoca con identidad y recurso
    /// ya conocidos; nunca incluye materiales del grant ni secretos.
    pub async fn record_issuance_denial(
        &self,
        installation_id: InstallationId,
        organization_id: OrganizationId,
        product_id: Uuid,
        actor: &CommercialActor,
        reason: &str,
        now: OffsetDateTime,
    ) -> AppResult<()> {
        self.repository
            .record_license_denial(
                installation_id,
                organization_id,
                product_id,
                actor.staff_account_id(),
                reason,
                now,
            )
            .await
    }

    /// Emite una licencia a partir de derechos comerciales ya verificados, o
    /// recupera el grant ya existente para el mismo par (instalación,
    /// suscripción).
    ///
    /// El plan, las capacidades y la vigencia provienen de [`EntitledPlan`], que
    /// a su vez sale de `CommercialService::entitlements`. La instalación debe
    /// pertenecer a la misma organización con derechos y el actor debe contar
    /// con la capacidad de gestionar instalaciones: la decisión comercial se
    /// comprueba aquí, no sólo en la capa HTTP.
    ///
    /// La recuperación reentrega el documento original sin firmar de nuevo ni
    /// crear un segundo grant; se audita como `license.recovered`, no como
    /// emisión. Ante una emisión concurrente (violación de unicidad), el
    /// perdedor recupera el grant del ganador: ambas solicitudes terminan con
    /// el mismo documento.
    pub async fn issue_entitled_license(
        &self,
        installation_id: InstallationId,
        entitled: EntitledPlan,
        actor: &CommercialActor,
        now: OffsetDateTime,
    ) -> AppResult<IssuedLicenseView> {
        if !actor.has_capability(Capability::InstallationsManage) {
            return Err(AppError::Unauthorized);
        }
        let signer = self.signer.as_ref().ok_or(AppError::LicensingDisabled)?;
        let installation = self
            .repository
            .get_installation(installation_id)
            .await?
            .ok_or(AppError::NotFound)?;
        // Una instalación de otra organización no revela si existe.
        if installation.organization_id != entitled.organization_id {
            return Err(AppError::NotFound);
        }
        IssuedLicense::validate_validity(now, entitled.valid_until)?;

        // Recuperación (B): el grant de esta suscripción ya fue emitido y
        // almacenado. Se reentrega tal cual, con el contexto comercial y el
        // actor revalidados arriba.
        if let Some(existing) = self
            .repository
            .find_license(installation_id, entitled.subscription_id)
            .await?
        {
            return self
                .recover_license(existing, entitled, actor, signer, now)
                .await;
        }

        let grant = LicenseGrant {
            version: LICENSE_DOC_VERSION,
            grant_id: GrantId::new(),
            installation_id: installation.id,
            organization_id: installation.organization_id,
            plan: entitled.plan.clone(),
            features: entitled.features.clone(),
            issued_at: now,
            valid_until: entitled.valid_until,
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
            plan: entitled.plan.clone(),
            features_json: serde_json::to_string(&entitled.features)
                .unwrap_or_else(|_| "[]".into()),
            issued_at: now,
            valid_until: entitled.valid_until,
            grant_json: grant_json.clone(),
            signature_b64: signature.clone(),
        };
        // Licencia y auditoría se escriben juntas: si la auditoría falla, no
        // queda una emisión registrada como exitosa. Una violación de unicidad
        // indica una emisión concurrente ganada por otra solicitud: el grant
        // ganador se recupera en lugar de reportar conflicto.
        match self
            .repository
            .record_license_with_audit(
                &issued,
                actor.staff_account_id(),
                entitled.product_id,
                entitled.subscription_id,
            )
            .await
        {
            Ok(()) => {}
            Err(AppError::Conflict) => {
                let existing = self
                    .repository
                    .find_license(installation_id, entitled.subscription_id)
                    .await?
                    .ok_or(AppError::Conflict)?;
                return self
                    .recover_license(existing, entitled, actor, signer, now)
                    .await;
            }
            Err(error) => return Err(error),
        }

        Ok(IssuedLicenseView {
            grant_id: issued.grant_id,
            installation_id: issued.installation_id,
            plan: issued.plan,
            valid_until: issued.valid_until,
            grant_json,
            signature_b64: signature,
            public_key_b64,
            recovered: false,
        })
    }

    /// Reentrega un grant ya emitido, con auditoría de recuperación.
    async fn recover_license(
        &self,
        existing: IssuedLicense,
        entitled: EntitledPlan,
        actor: &CommercialActor,
        signer: &SharedSigner,
        now: OffsetDateTime,
    ) -> AppResult<IssuedLicenseView> {
        self.repository
            .record_license_recovery(
                &existing,
                actor.staff_account_id(),
                entitled.product_id,
                entitled.subscription_id,
                now,
            )
            .await?;
        Ok(IssuedLicenseView {
            grant_id: existing.grant_id,
            installation_id: existing.installation_id,
            plan: existing.plan,
            valid_until: existing.valid_until,
            grant_json: existing.grant_json,
            signature_b64: existing.signature_b64,
            public_key_b64: signer.public_key_b64(),
            recovered: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_domain::CommercialActor;
    use crate::ports::{LicenseSigner, PlatformRepository, SharedRepository, SharedSigner};
    use async_trait::async_trait;
    use platform_core::ids::StaffAccountId;
    use platform_core::installation::InstallationStatus;
    use platform_core::staff::StaffRole;
    use std::sync::Mutex;

    /// Repositorio de prueba: sólo registra las llamadas relevantes y falla
    /// (pánico) si la emisión consulta algo que el flujo no debe consultar.
    struct StubRepository {
        installation: Option<Installation>,
        audit_fails: bool,
        recorded: std::sync::Arc<Mutex<Vec<IssuedLicense>>>,
        existing: Mutex<Vec<IssuedLicense>>,
        recoveries: std::sync::Arc<Mutex<Vec<Uuid>>>,
    }

    impl StubRepository {
        fn assertion() -> Self {
            Self {
                installation: None,
                audit_fails: false,
                recorded: std::sync::Arc::new(Mutex::new(Vec::new())),
                existing: Mutex::new(Vec::new()),
                recoveries: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_installation(mut self, organization_id: OrganizationId) -> Self {
            let mut installation = Installation::new(
                InstallationId::new(),
                organization_id,
                "fixture".to_owned(),
                time::OffsetDateTime::now_utc(),
            )
            .unwrap();
            installation.status = InstallationStatus::Active;
            self.installation = Some(installation);
            self
        }

        fn with_failing_audit(mut self) -> Self {
            self.audit_fails = true;
            self
        }
    }

    #[async_trait]
    impl PlatformRepository for StubRepository {
        async fn list_plans(&self) -> AppResult<Vec<PlanDefinition>> {
            panic!("la emisión no debe consultar el catálogo técnico")
        }

        async fn register_installation(&self, _: &Installation) -> AppResult<Installation> {
            panic!("no aplica")
        }

        async fn get_installation(&self, _: InstallationId) -> AppResult<Option<Installation>> {
            Ok(self.installation.clone())
        }

        async fn start_trial(
            &self,
            _: InstallationId,
            _: OffsetDateTime,
        ) -> AppResult<Installation> {
            panic!("no aplica")
        }

        async fn record_license(&self, _: &IssuedLicense) -> AppResult<()> {
            panic!("la emisión comercial no debe usar el registro DEV")
        }

        async fn find_license(
            &self,
            _: InstallationId,
            _: Uuid,
        ) -> AppResult<Option<IssuedLicense>> {
            Ok(self.existing.lock().unwrap().first().cloned())
        }

        async fn record_license_with_audit(
            &self,
            license: &IssuedLicense,
            _: StaffAccountId,
            _: Uuid,
            _: Uuid,
        ) -> AppResult<()> {
            if self.audit_fails {
                return Err(AppError::Storage);
            }
            self.recorded.lock().unwrap().push(license.clone());
            Ok(())
        }

        async fn record_license_recovery(
            &self,
            license: &IssuedLicense,
            _: StaffAccountId,
            _: Uuid,
            _: Uuid,
            _: OffsetDateTime,
        ) -> AppResult<()> {
            self.recoveries
                .lock()
                .unwrap()
                .push(license.grant_id.as_uuid());
            Ok(())
        }

        async fn record_license_denial(
            &self,
            _: InstallationId,
            _: OrganizationId,
            _: Uuid,
            _: StaffAccountId,
            _: &str,
            _: OffsetDateTime,
        ) -> AppResult<()> {
            Ok(())
        }

        async fn list_licenses(&self, _: InstallationId) -> AppResult<Vec<IssuedLicense>> {
            panic!("no aplica")
        }
    }

    struct StaticSigner;

    impl LicenseSigner for StaticSigner {
        fn sign(&self, grant_json: &str) -> AppResult<String> {
            // Firma sintética: sólo importa que la emisión produzca material.
            Ok(format!("firma-de-{}-bytes", grant_json.len()))
        }

        fn public_key_b64(&self) -> String {
            "clave-publica-sintetica".to_owned()
        }
    }

    fn service(repository: StubRepository, signer: Option<SharedSigner>) -> PlatformService {
        PlatformService::new(std::sync::Arc::new(repository) as SharedRepository, signer)
    }

    fn entitled() -> EntitledPlan {
        EntitledPlan {
            organization_id: OrganizationId::new(),
            product_id: Uuid::new_v4(),
            subscription_id: Uuid::new_v4(),
            plan: "pro".to_owned(),
            features: vec!["catalog".to_owned(), "sales".to_owned()],
            valid_until: time::OffsetDateTime::now_utc() + time::Duration::days(45),
            is_trial: false,
        }
    }

    fn manager() -> CommercialActor {
        CommercialActor::new(StaffAccountId::new(), StaffRole::PlatformAdmin)
    }

    fn read_only_actor() -> CommercialActor {
        CommercialActor::new(StaffAccountId::new(), StaffRole::Support)
    }

    #[tokio::test]
    async fn sin_capacidad_no_consulta_nada() {
        let repository = StubRepository::assertion().with_installation(OrganizationId::new());
        let service = service(repository, None);
        let error = service
            .issue_entitled_license(InstallationId::new(), entitled(), &read_only_actor(), now())
            .await
            .unwrap_err();
        assert_eq!(error, AppError::Unauthorized);
    }

    #[tokio::test]
    async fn instalacion_ajena_no_revela_nada_y_no_emite() {
        let repository = StubRepository::assertion().with_installation(OrganizationId::new());
        let service = service(repository, Some(signer_arc()));
        let error = service
            .issue_entitled_license(InstallationId::new(), entitled(), &manager(), now())
            .await
            .unwrap_err();
        // 404, no 403: la existencia de una instalación de otra organización no
        // se revela a un actor autorizado de ésta.
        assert_eq!(error, AppError::NotFound);
    }

    #[tokio::test]
    async fn sin_firmante_esta_deshabilitada() {
        let organization = OrganizationId::new();
        let repository = StubRepository::assertion().with_installation(organization);
        let service = service(repository, None);
        let mut right = entitled();
        right.organization_id = organization;
        let error = service
            .issue_entitled_license(InstallationId::new(), right, &manager(), now())
            .await
            .unwrap_err();
        assert_eq!(error, AppError::LicensingDisabled);
    }

    #[tokio::test]
    async fn auditoria_fallida_no_deja_emision_exitosa() {
        let organization = OrganizationId::new();
        let repository = StubRepository::assertion()
            .with_installation(organization)
            .with_failing_audit();
        let repository = std::sync::Arc::new(repository);
        let recorded = repository.recorded.clone();
        let service = PlatformService::new(repository as SharedRepository, Some(signer_arc()));
        let mut right = entitled();
        right.organization_id = organization;
        let error = service
            .issue_entitled_license(InstallationId::new(), right, &manager(), now())
            .await
            .unwrap_err();
        assert_eq!(error, AppError::Storage);
        assert!(recorded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn emision_valida_firma_los_derechos_resueltos() {
        let organization = OrganizationId::new();
        let repository = StubRepository::assertion().with_installation(organization);
        let repository = std::sync::Arc::new(repository);
        let recorded = repository.recorded.clone();
        let service = PlatformService::new(repository as SharedRepository, Some(signer_arc()));
        let mut right = entitled();
        right.organization_id = organization;
        let view = service
            .issue_entitled_license(InstallationId::new(), right.clone(), &manager(), now())
            .await
            .unwrap();
        assert_eq!(view.plan, "pro");
        assert_eq!(view.valid_until, right.valid_until);
        assert!(!view.signature_b64.is_empty());
        assert_eq!(view.public_key_b64, "clave-publica-sintetica");

        let grant: LicenseGrant = serde_json::from_str(&view.grant_json).unwrap();
        assert_eq!(grant.version, LICENSE_DOC_VERSION);
        assert_eq!(grant.organization_id, organization);
        assert_eq!(grant.plan, "pro");
        assert_eq!(grant.features, right.features);
        assert_eq!(grant.valid_until, right.valid_until);
        assert_eq!(recorded.lock().unwrap().len(), 1);
    }

    fn now() -> OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }

    fn signer_arc() -> SharedSigner {
        std::sync::Arc::new(StaticSigner) as SharedSigner
    }

    fn grant_previo(installation: InstallationId, organization: OrganizationId) -> IssuedLicense {
        IssuedLicense {
            grant_id: GrantId::new(),
            installation_id: installation,
            organization_id: organization,
            plan: "pro".to_owned(),
            features_json: "[\"catalog\"]".to_owned(),
            issued_at: now(),
            valid_until: now() + time::Duration::days(45),
            grant_json: "{\"grant_json_original\":true}".to_owned(),
            signature_b64: "firma-original".to_owned(),
        }
    }

    #[tokio::test]
    async fn recuperacion_reentrega_el_grant_original_sin_nueva_emision() {
        let organization = OrganizationId::new();
        let repository = StubRepository::assertion().with_installation(organization);
        let repository = std::sync::Arc::new(repository);
        let (recorded, recoveries) = (repository.recorded.clone(), repository.recoveries.clone());
        {
            let mut existing = repository.existing.lock().unwrap();
            existing.push(grant_previo(InstallationId::new(), organization));
        }
        let service = PlatformService::new(repository as SharedRepository, Some(signer_arc()));
        let mut right = entitled();
        right.organization_id = organization;
        let view = service
            .issue_entitled_license(InstallationId::new(), right, &manager(), now())
            .await
            .unwrap();
        // El documento original, sin re-firmar: ni un segundo grant ni una
        // emisión nueva contabilizada.
        assert!(view.recovered);
        assert_eq!(view.grant_json, "{\"grant_json_original\":true}");
        assert_eq!(view.signature_b64, "firma-original");
        assert!(recorded.lock().unwrap().is_empty());
        assert_eq!(recoveries.lock().unwrap().len(), 1);
    }
}
