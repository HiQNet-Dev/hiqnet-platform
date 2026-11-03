use async_trait::async_trait;
use platform_application::{AppError, AppResult, PlatformRepository};
use platform_core::ids::InstallationId;
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
        .map_err(|_| AppError::Storage)?;

        if result.is_none() {
            let existing = sqlx::query(
                "SELECT id, organization_id, device_label, status, registered_at, last_seen_at,\n                        trial_started_at, trial_ends_at\n                 FROM installation WHERE id = $1",
            )
            .bind(installation.id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AppError::Storage)?;
            return Ok(row_to_installation(existing));
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
        sqlx::query(
            "UPDATE installation\n             SET trial_started_at = $2, trial_ends_at = $3, last_seen_at = $4\n             WHERE id = $1 AND trial_started_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(installation.trial_started_at)
        .bind(installation.trial_ends_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;
        Ok(installation)
    }

    async fn record_license(&self, license: &IssuedLicense) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO issued_license\n             (grant_id, installation_id, organization_id, plan, features_json,\n              issued_at, valid_until, grant_json, signature_b64)\n             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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

    async fn list_licenses(
        &self,
        installation_id: InstallationId,
    ) -> AppResult<Vec<IssuedLicense>> {
        let rows = sqlx::query(
            "SELECT grant_id, installation_id, organization_id, plan, features_json,\n                    issued_at, valid_until, grant_json, signature_b64\n             FROM issued_license\n             WHERE installation_id = $1\n             ORDER BY issued_at DESC",
        )
        .bind(installation_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AppError::Storage)?;

        rows.into_iter()
            .map(|row| {
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
            })
            .collect()
    }
}

fn row_to_installation(row: sqlx::postgres::PgRow) -> Installation {
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
