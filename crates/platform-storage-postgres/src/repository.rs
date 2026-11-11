use async_trait::async_trait;
use platform_application::{AppError, AppResult, PlatformRepository};
use platform_core::ids::{InstallationId, OrganizationId, StaffAccountId};
use platform_core::installation::{Installation, InstallationStatus};
use platform_core::license::IssuedLicense;
use platform_core::plan::PlanDefinition;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PgRepository {
    pool: PgPool,
}

impl PgRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl PlatformRepository for PgRepository {
    async fn list_plans(&self) -> AppResult<Vec<PlanDefinition>> {
        Ok(PlanDefinition::catalog())
    }

    async fn register_installation(&self, installation: &Installation) -> AppResult<Installation> {
        let status = installation.status.as_str();
        let result = sqlx::query(
            "INSERT INTO installation \n             (id, organization_id, device_label, status, registered_at, last_seen_at,\n              trial_started_at, trial_ends_at)\n             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)\n             ON CONFLICT (id) DO NOTHING\n             RETURNING id",
        )
        .bind(installation.id.as_uuid())
        .bind(installation.organization_id.as_uuid())
        .bind(&installation.device_label)
        .bind(status)
        .bind(installation.registered_at)
        .bind(installation.last_seen_at)
        .bind(installation.trial_started_at)
        .bind(installation.trial_ends_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict,
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => AppError::NotFound,
            _ => AppError::Storage,
        })?;

        if result.is_none() {
            let existing = sqlx::query(
                "SELECT id, organization_id, device_label, status, registered_at, last_seen_at,\n                        trial_started_at, trial_ends_at\n                 FROM installation WHERE id = $1",
            )
            .bind(installation.id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AppError::Storage)?;
            let existing = row_to_installation(existing);
            if existing.organization_id != installation.organization_id {
                return Err(AppError::Conflict);
            }
            return Ok(existing);
        }

        Ok(installation.clone())
    }

    async fn get_installation(&self, id: InstallationId) -> AppResult<Option<Installation>> {
        let row = sqlx::query(
            "SELECT id, organization_id, device_label, status, registered_at, last_seen_at,\n                    trial_started_at, trial_ends_at\n             FROM installation WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        Ok(row.map(row_to_installation))
    }

    async fn start_trial(
        &self,
        id: InstallationId,
        now: OffsetDateTime,
    ) -> AppResult<Installation> {
        let mut installation = self.get_installation(id).await?.ok_or(AppError::NotFound)?;
        installation.start_trial(now).map_err(AppError::Domain)?;
        let result = sqlx::query(
            "UPDATE installation\n             SET trial_started_at = $2, trial_ends_at = $3, last_seen_at = $4\n             WHERE id = $1 AND trial_started_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(installation.trial_started_at)
        .bind(installation.trial_ends_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        if result.rows_affected() == 0 {
            return Err(AppError::Conflict);
        }
        Ok(installation)
    }

    async fn record_license(&self, license: &IssuedLicense) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO issued_license\n             (grant_id, installation_id, organization_id, plan, features_json,\n              issued_at, valid_until, grant_json, signature_b64)\n             VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, $8, $9)",
        )
        .bind(license.grant_id.as_uuid())
        .bind(license.installation_id.as_uuid())
        .bind(license.organization_id.as_uuid())
        .bind(&license.plan)
        .bind(&license.features_json)
        .bind(license.issued_at)
        .bind(license.valid_until)
        .bind(&license.grant_json)
        .bind(&license.signature_b64)
        .execute(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        Ok(())
    }

    async fn record_license_with_audit(
        &self,
        license: &IssuedLicense,
        actor: StaffAccountId,
        product_id: Uuid,
        subscription_id: Uuid,
    ) -> AppResult<()> {
        // Licencia y auditoría comparten transacción: si algo falla no queda una
        // emisión registrada como exitosa sin su evento.
        let mut tx = self.pool.begin().await.map_err(|_| AppError::Storage)?;
        sqlx::query(
            "INSERT INTO issued_license\n             (grant_id, installation_id, organization_id, plan, features_json,\n              issued_at, valid_until, grant_json, signature_b64, subscription_id)\n             VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, $8, $9, $10)",
        )
        .bind(license.grant_id.as_uuid())
        .bind(license.installation_id.as_uuid())
        .bind(license.organization_id.as_uuid())
        .bind(&license.plan)
        .bind(&license.features_json)
        .bind(license.issued_at)
        .bind(license.valid_until)
        .bind(&license.grant_json)
        .bind(&license.signature_b64)
        .bind(subscription_id)
        .execute(&mut *tx)
        .await
        .map_err(classify_issuance_insert)?;

        sqlx::query(
            "INSERT INTO staff_audit_event\n             (id, staff_account_id, action, resource_type, resource_id, organization_id,\n              product_id, result, occurred_at, metadata)\n             VALUES ($1, $2, 'license.issued', 'license', $3, $4, $5, 'success', $6, $7::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(actor.as_uuid())
        .bind(license.grant_id.as_uuid())
        .bind(license.organization_id.as_uuid())
        .bind(product_id)
        .bind(license.issued_at)
        .bind(
            serde_json::json!({
                "installation_id": license.installation_id.as_uuid().to_string(),
                "subscription_id": subscription_id.to_string(),
                "plan": license.plan,
                "valid_until": license
                    .valid_until
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            })
            .to_string(),
        )
        .execute(&mut *tx)
        .await
        .map_err(classify_issuance_insert)?;

        tx.commit().await.map_err(|_| AppError::Storage)?;
        Ok(())
    }

    async fn record_license_denial(
        &self,
        installation_id: InstallationId,
        organization_id: OrganizationId,
        product_id: Uuid,
        actor: StaffAccountId,
        reason: &str,
        now: OffsetDateTime,
    ) -> AppResult<()> {
        // El rechazo se audita en evento propio: nunca deja una emisión
        // registrada y no contiene materiales del grant.
        sqlx::query(
            "INSERT INTO staff_audit_event\n             (id, staff_account_id, action, resource_type, resource_id, organization_id,\n              product_id, result, occurred_at, metadata)\n             VALUES ($1, $2, 'license.issuance_denied', 'license', NULL, $3, $4, 'denied', $5, $6::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(actor.as_uuid())
        .bind(organization_id.as_uuid())
        .bind(product_id)
        .bind(now)
        .bind(
            serde_json::json!({
                "installation_id": installation_id.as_uuid().to_string(),
                "reason": reason,
            })
            .to_string(),
        )
        .execute(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        Ok(())
    }

    async fn find_license(
        &self,
        installation_id: InstallationId,
        subscription_id: Uuid,
    ) -> AppResult<Option<IssuedLicense>> {
        // La recuperación usa el mismo índice parcial único de la emisión: el
        // grant original se reentrega, nunca se genera otro.
        let row = sqlx::query(
            "SELECT grant_id, installation_id, organization_id, plan, features_json::text AS features_json,\n                    issued_at, valid_until, grant_json, signature_b64\n             FROM issued_license\n             WHERE installation_id = $1 AND subscription_id = $2",
        )
        .bind(installation_id.as_uuid())
        .bind(subscription_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        row.map(license_row).transpose()
    }

    async fn record_license_recovery(
        &self,
        license: &IssuedLicense,
        actor: StaffAccountId,
        product_id: Uuid,
        subscription_id: Uuid,
        now: OffsetDateTime,
    ) -> AppResult<()> {
        // Evento propio de recuperación: no se contabiliza como emisión nueva y
        // no crea ni modifica el registro del grant.
        sqlx::query(
            "INSERT INTO staff_audit_event\n             (id, staff_account_id, action, resource_type, resource_id, organization_id,\n              product_id, result, occurred_at, metadata)\n             VALUES ($1, $2, 'license.recovered', 'license', $3, $4, $5, 'success', $6, $7::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(actor.as_uuid())
        .bind(license.grant_id.as_uuid())
        .bind(license.organization_id.as_uuid())
        .bind(product_id)
        .bind(now)
        .bind(
            serde_json::json!({
                "installation_id": license.installation_id.as_uuid().to_string(),
                "subscription_id": subscription_id.to_string(),
                "plan": license.plan,
                "valid_until": license
                    .valid_until
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            })
            .to_string(),
        )
        .execute(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        Ok(())
    }

    async fn list_licenses(
        &self,
        installation_id: InstallationId,
    ) -> AppResult<Vec<IssuedLicense>> {
        let rows = sqlx::query(
            "SELECT grant_id, installation_id, organization_id, plan, features_json::text AS features_json,\n                    issued_at, valid_until, grant_json, signature_b64\n             FROM issued_license\n             WHERE installation_id = $1\n             ORDER BY issued_at DESC",
        )
        .bind(installation_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;

        rows.into_iter().map(license_row).collect()
    }
}

fn license_row(row: sqlx::postgres::PgRow) -> AppResult<IssuedLicense> {
    Ok(IssuedLicense {
        grant_id: row.get::<Uuid, _>("grant_id").into(),
        installation_id: row.get::<Uuid, _>("installation_id").into(),
        organization_id: row.get::<Uuid, _>("organization_id").into(),
        plan: row.get("plan"),
        features_json: row.get("features_json"),
        issued_at: row.get("issued_at"),
        valid_until: row.get("valid_until"),
        grant_json: row.get("grant_json"),
        signature_b64: row.get("signature_b64"),
    })
}

/// La emisión entra con la unicidad (instalación, suscripción) garantizada por
/// índice parcial: una emisión duplicada es un conflicto de negocio, no un
/// fallo de almacenamiento.
fn classify_issuance_insert(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => AppError::Conflict,
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => AppError::NotFound,
        _ => AppError::Storage,
    }
}

pub(crate) fn row_to_installation(row: sqlx::postgres::PgRow) -> Installation {
    let status = match row.get::<String, _>("status").as_str() {
        "suspended" => InstallationStatus::Suspended,
        "unregistered" => InstallationStatus::Unregistered,
        _ => InstallationStatus::Active,
    };
    Installation {
        id: row.get::<Uuid, _>("id").into(),
        organization_id: row.get::<Uuid, _>("organization_id").into(),
        device_label: row.get("device_label"),
        status,
        registered_at: row.get("registered_at"),
        last_seen_at: row.get("last_seen_at"),
        trial_started_at: row.get("trial_started_at"),
        trial_ends_at: row.get("trial_ends_at"),
    }
}
