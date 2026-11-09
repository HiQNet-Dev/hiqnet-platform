//! Persistencia del control administrativo de instalaciones POS.

use async_trait::async_trait;
use platform_application::installation_control::{
    AdministrativeStatus, ControlActor, ControlCursor, CurrentBindingView,
    InstallationControlError, InstallationControlRepository, InstallationControlResult,
    InstallationDetail, InstallationFilters, InstallationSummary, RehostAuthorizationView,
    RehostStatus,
};
use platform_core::ids::{
    CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId, ProductId,
};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgInstallationControlRepository {
    pool: PgPool,
}

impl PgInstallationControlRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> InstallationControlError {
    InstallationControlError::Storage
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const SUMMARY_SELECT: &str = "SELECT i.id, i.device_label, i.organization_id, \
    o.display_name AS organization_display_name, o.status AS organization_status, \
    i.product_id, p.key AS product_key, p.name AS product_name, \
    i.status AS administrative_status, i.identity_status, i.registered_at, i.enrolled_at, \
    i.last_seen_at, idn.generation, idn.fingerprint, idn.created_at AS identity_created_at, \
    (b.id IS NOT NULL) AS binding_active \
    FROM installation i \
    JOIN organization o ON o.id = i.organization_id \
    LEFT JOIN product p ON p.id = i.product_id \
    LEFT JOIN installation_identity idn ON idn.installation_id = i.id AND idn.retired_at IS NULL \
    LEFT JOIN installation_binding b ON b.installation_id = i.id AND b.revoked_at IS NULL";

fn summary_from_row(row: &sqlx::postgres::PgRow) -> InstallationControlResult<InstallationSummary> {
    let administrative_status: String = row.get("administrative_status");
    let identity_status: String = row.get("identity_status");
    let product_id: Option<Uuid> = row.get("product_id");
    let fingerprint: Option<Vec<u8>> = row.get("fingerprint");
    let generation: Option<i32> = row.get("generation");
    Ok(InstallationSummary {
        id: InstallationId::from_uuid(row.get("id")),
        device_label: row.get("device_label"),
        organization_id: OrganizationId::from_uuid(row.get("organization_id")),
        organization_display_name: row.get("organization_display_name"),
        organization_status: row.get("organization_status"),
        product_id: product_id.map(ProductId::from_uuid),
        product_key: row.get("product_key"),
        product_name: row.get("product_name"),
        administrative_status: administrative_status.parse()?,
        identity_status: identity_status.parse()?,
        registered_at: row.get("registered_at"),
        enrolled_at: row.get("enrolled_at"),
        last_known_activity_at: row.get("last_seen_at"),
        identity_generation: generation.map(|value| value as u32),
        binding_active: row.get("binding_active"),
        fingerprint: fingerprint.as_deref().map(hex),
    })
}

fn rehost_from_row(
    row: &sqlx::postgres::PgRow,
) -> InstallationControlResult<RehostAuthorizationView> {
    let status: String = row.get("status");
    Ok(RehostAuthorizationView {
        id: row.get("id"),
        enrollment_request_id: EnrollmentRequestId::from_uuid(row.get("enrollment_request_id")),
        status: status.parse()?,
        reason: row.get("reason"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    })
}

#[allow(clippy::too_many_arguments)]
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: &ControlActor,
    action: &str,
    installation: InstallationId,
    organization: OrganizationId,
    product: Option<ProductId>,
    metadata: serde_json::Value,
    now: OffsetDateTime,
) -> InstallationControlResult<()> {
    sqlx::query(
        "INSERT INTO staff_audit_event \
         (id, staff_account_id, action, resource_type, resource_id, organization_id, product_id, \
          result, occurred_at, metadata) \
         VALUES ($1, $2, $3, 'installation', $4, $5, $6, 'success', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(actor.account_id.as_uuid())
    .bind(action)
    .bind(installation.as_uuid())
    .bind(organization.as_uuid())
    .bind(product.map(|value| value.as_uuid()))
    .bind(now)
    .bind(metadata)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

async fn installation_detail(
    pool: &PgPool,
    id: InstallationId,
) -> InstallationControlResult<Option<InstallationDetail>> {
    let row = sqlx::query(&format!("{SUMMARY_SELECT} WHERE i.id = $1"))
        .bind(id.as_uuid())
        .fetch_optional(pool)
        .await
        .map_err(storage)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let summary = summary_from_row(&row)?;
    let current_identity_created_at: Option<OffsetDateTime> = row.get("identity_created_at");
    let binding = sqlx::query(
        "SELECT bound_at, approved_by_account_id FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(storage)?;
    let current_binding = binding.map(|row| CurrentBindingView {
        bound_at: row.get("bound_at"),
        approved_by_account_id: CommercialAccountId::from_uuid(row.get("approved_by_account_id")),
    });
    let rehost = sqlx::query(
        "SELECT id, enrollment_request_id, status, reason, created_at, expires_at \
         FROM installation_rehost_authorization \
         WHERE installation_id = $1 AND status = 'pending' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(storage)?;
    let rehost = rehost.as_ref().map(rehost_from_row).transpose()?;
    Ok(Some(InstallationDetail {
        summary,
        current_identity_created_at,
        current_binding,
        rehost,
    }))
}

#[async_trait]
impl InstallationControlRepository for PgInstallationControlRepository {
    async fn list_installations(
        &self,
        filters: &InstallationFilters,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> InstallationControlResult<Vec<InstallationSummary>> {
        let mut query = QueryBuilder::<Postgres>::new(SUMMARY_SELECT);
        query.push(" WHERE 1 = 1");
        if let Some(status) = filters.status {
            query.push(" AND i.status = ").push_bind(status.as_str());
        }
        if let Some(status) = filters.identity_status {
            query
                .push(" AND i.identity_status = ")
                .push_bind(status.as_str());
        }
        if let Some(organization) = filters.organization_id {
            query
                .push(" AND i.organization_id = ")
                .push_bind(organization.as_uuid());
        }
        if let Some(product) = filters.product_id {
            query
                .push(" AND i.product_id = ")
                .push_bind(product.as_uuid());
        }
        if let Some(search) = filters.search.as_deref() {
            query
                .push(" AND i.device_label ILIKE ")
                .push_bind(format!("%{}%", escape_like(search)))
                .push(" ESCAPE '\\'");
        }
        if let Some(cursor) = after {
            query
                .push(" AND (i.registered_at, i.id) > (")
                .push_bind(cursor.registered_at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        query
            .push(" ORDER BY i.registered_at ASC, i.id ASC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(&self.pool).await.map_err(storage)?;
        rows.iter().map(summary_from_row).collect()
    }

    async fn get_installation(
        &self,
        id: InstallationId,
    ) -> InstallationControlResult<Option<InstallationDetail>> {
        installation_detail(&self.pool, id).await
    }

    async fn set_administrative_status(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        target: AdministrativeStatus,
        now: OffsetDateTime,
    ) -> InstallationControlResult<InstallationDetail> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "SELECT status, organization_id, product_id FROM installation WHERE id = $1 FOR UPDATE",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::NotFound);
        };
        let current: String = row.get("status");
        let organization = OrganizationId::from_uuid(row.get("organization_id"));
        let product: Option<Uuid> = row.get("product_id");
        if current == target.as_str() {
            tx.rollback().await.map_err(storage)?;
            return installation_detail(&self.pool, id)
                .await?
                .ok_or(InstallationControlError::NotFound);
        }
        let allowed = matches!(
            (current.as_str(), target),
            ("active", AdministrativeStatus::Suspended)
                | ("suspended", AdministrativeStatus::Active)
        );
        if !allowed {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::TransitionConflict);
        }
        sqlx::query("UPDATE installation SET status = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(target.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let action = match target {
            AdministrativeStatus::Suspended => "installation_suspended",
            AdministrativeStatus::Active => "installation_reactivated",
            AdministrativeStatus::Unregistered => unreachable!(),
        };
        audit(
            &mut tx,
            actor,
            action,
            id,
            organization,
            product.map(ProductId::from_uuid),
            serde_json::json!({ "before": current, "after": target.as_str() }),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        installation_detail(&self.pool, id)
            .await?
            .ok_or(InstallationControlError::NotFound)
    }

    async fn authorize_rehost(
        &self,
        actor: &ControlActor,
        id: InstallationId,
        enrollment_request_id: EnrollmentRequestId,
        reason: &str,
        now: OffsetDateTime,
    ) -> InstallationControlResult<RehostAuthorizationView> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Lock order: primero el enrollment_request concreto y sólo después se
        // reconsulta la autorización. Dos reintentos del mismo request se
        // serializan aquí y devuelven la misma autorización.
        let request = sqlx::query(
            "SELECT installation_id, product_id, status, organization_id, \
                    approved_by_account_id, expires_at \
             FROM enrollment_request WHERE id = $1 FOR UPDATE",
        )
        .bind(enrollment_request_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(request) = request else {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::RehostConflict);
        };

        let existing = sqlx::query(
            "SELECT id, installation_id, enrollment_request_id, status, reason, created_at, \
                    expires_at \
             FROM installation_rehost_authorization WHERE enrollment_request_id = $1",
        )
        .bind(enrollment_request_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if let Some(row) = existing {
            let stored_installation: Uuid = row.get("installation_id");
            if stored_installation != id.as_uuid() {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::RehostConflict);
            }
            let view = rehost_from_row(&row)?;
            return match view.status {
                RehostStatus::Pending | RehostStatus::Consumed => {
                    tx.commit().await.map_err(storage)?;
                    Ok(view)
                }
                _ => {
                    tx.rollback().await.map_err(storage)?;
                    Err(InstallationControlError::RehostConflict)
                }
            };
        }

        let request_status: String = request.get("status");
        let request_installation: Uuid = request.get("installation_id");
        let request_organization: Option<Uuid> = request.get("organization_id");
        let request_approved_by: Option<Uuid> = request.get("approved_by_account_id");
        let request_product: Option<Uuid> = request.get("product_id");
        let request_expires: OffsetDateTime = request.get("expires_at");
        if request_status != "approved"
            || request_installation != id.as_uuid()
            || request_organization.is_none()
            || request_approved_by.is_none()
            || request_expires <= now
        {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::RehostConflict);
        }
        let organization = OrganizationId::from_uuid(request_organization.unwrap());

        let installation = sqlx::query(
            "SELECT status, identity_status, enrolled_at, organization_id, product_id \
             FROM installation WHERE id = $1 FOR UPDATE",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(installation) = installation else {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::NotFound);
        };
        let administrative: String = installation.get("status");
        let identity: String = installation.get("identity_status");
        let enrolled_at: Option<OffsetDateTime> = installation.get("enrolled_at");
        let installation_organization: Uuid = installation.get("organization_id");
        let installation_product: Option<Uuid> = installation.get("product_id");
        if !matches!(administrative.as_str(), "active" | "suspended")
            || installation_organization != organization.as_uuid()
        {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::TransitionConflict);
        }
        // `active` exige cutover; `unenrolled` sólo es reintentable si la
        // instalación ya estuvo enrolada y no conserva identidad/binding activos.
        let cutover = match identity.as_str() {
            "active" => true,
            "unenrolled" if enrolled_at.is_some() => false,
            _ => {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::TransitionConflict);
            }
        };
        let effective_product = match (request_product, installation_product) {
            (Some(requested), Some(installed)) if requested != installed => {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::RehostConflict);
            }
            (Some(requested), _) => requested,
            (None, Some(installed)) => installed,
            (None, None) => {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::ProductUnavailable);
            }
        };
        let product_id = ProductId::from_uuid(effective_product);
        let product_status: String = sqlx::query_scalar("SELECT status FROM product WHERE id = $1")
            .bind(effective_product)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
            .ok_or(InstallationControlError::ProductUnavailable)?;
        if !matches!(product_status.as_str(), "active" | "maintenance") {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::ProductUnavailable);
        }
        let association: Option<Uuid> = sqlx::query_scalar(
            "SELECT product_id FROM organization_product \
             WHERE organization_id = $1 AND product_id = $2 AND status = 'active'",
        )
        .bind(organization.as_uuid())
        .bind(effective_product)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if association.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::ProductUnavailable);
        }

        sqlx::query(
            "UPDATE installation_rehost_authorization SET status = 'expired' \
             WHERE installation_id = $1 AND status = 'pending' AND expires_at <= $2",
        )
        .bind(id.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        let other_pending: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM installation_rehost_authorization \
             WHERE installation_id = $1 AND status = 'pending' FOR UPDATE",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if other_pending.is_some() {
            tx.rollback().await.map_err(storage)?;
            return Err(InstallationControlError::RehostConflict);
        }

        let authorization_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO installation_rehost_authorization \
             (id, installation_id, enrollment_request_id, organization_id, product_id, \
              authorized_by_staff_account_id, reason, status, created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9)",
        )
        .bind(authorization_id)
        .bind(id.as_uuid())
        .bind(enrollment_request_id.as_uuid())
        .bind(organization.as_uuid())
        .bind(effective_product)
        .bind(actor.account_id.as_uuid())
        .bind(reason)
        .bind(now)
        .bind(request_expires)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        if cutover {
            let retired = sqlx::query(
                "UPDATE installation_identity SET retired_at = $2 \
                 WHERE installation_id = $1 AND retired_at IS NULL",
            )
            .bind(id.as_uuid())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            if retired.rows_affected() != 1 {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::TransitionConflict);
            }
            let revoked = sqlx::query(
                "UPDATE installation_binding SET revoked_at = $2, revoke_reason = 'rehost' \
                 WHERE installation_id = $1 AND revoked_at IS NULL",
            )
            .bind(id.as_uuid())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            if revoked.rows_affected() != 1 {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::TransitionConflict);
            }
        } else {
            let active_identity: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM installation_identity \
                 WHERE installation_id = $1 AND retired_at IS NULL",
            )
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?;
            let active_binding: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM installation_binding \
                 WHERE installation_id = $1 AND revoked_at IS NULL",
            )
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?;
            if active_identity.is_some() || active_binding.is_some() {
                tx.rollback().await.map_err(storage)?;
                return Err(InstallationControlError::TransitionConflict);
            }
        }
        sqlx::query(
            "UPDATE installation SET identity_status = 'unenrolled', \
             product_id = COALESCE(product_id, $2) WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(effective_product)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        audit(
            &mut tx,
            actor,
            "installation_rehost_authorized",
            id,
            organization,
            Some(product_id),
            serde_json::json!({
                "enrollment_request_id": enrollment_request_id.to_string(),
                "reason": reason,
                "expires_at": request_expires.to_string(),
            }),
            now,
        )
        .await?;
        audit(
            &mut tx,
            actor,
            "installation_identity_retired",
            id,
            organization,
            Some(product_id),
            serde_json::json!({ "generation": "active" }),
            now,
        )
        .await?;
        audit(
            &mut tx,
            actor,
            "installation_binding_revoked",
            id,
            organization,
            Some(product_id),
            serde_json::json!({ "reason": "rehost" }),
            now,
        )
        .await?;

        sqlx::query(
            "INSERT INTO commercial_security_event \
             (id, kind, organization_id, account_id, installation_id, metadata, created_at) \
             VALUES ($1, 'identity_revoked', $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(organization.as_uuid())
        .bind(request_approved_by)
        .bind(id.as_uuid())
        .bind(serde_json::json!({ "reason": reason }))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        tx.commit().await.map_err(storage)?;
        Ok(RehostAuthorizationView {
            id: authorization_id,
            enrollment_request_id,
            status: RehostStatus::Pending,
            reason: reason.to_owned(),
            created_at: now,
            expires_at: request_expires,
        })
    }
}
