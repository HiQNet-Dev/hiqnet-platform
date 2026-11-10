//! Autoridad bloqueada, auditoría y lecturas de apoyo.

use super::*;

pub(super) async fn locked_authority(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
) -> EnrollmentResult<()> {
    let active_org: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM organization WHERE id = $1 AND status = 'active' FOR UPDATE",
    )
    .bind(organization.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    if active_org.is_none() {
        return Err(EnrollmentError::Forbidden);
    }

    // FOR SHARE bloquea el UPDATE de status (FOR NO KEY UPDATE) de una
    // deshabilitación concurrente; FOR KEY SHARE no lo haría.
    let active_account: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM commercial_account \
         WHERE id = $1 AND status = 'active' FOR SHARE",
    )
    .bind(actor.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    if active_account.is_none() {
        return Err(EnrollmentError::Forbidden);
    }

    let membership: Option<(String, i16)> = sqlx::query_as(
        "SELECT role, role_version FROM organization_membership \
         WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL",
    )
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    match membership {
        Some((role, 1)) if role == "owner" => Ok(()),
        _ => Err(EnrollmentError::Forbidden),
    }
}

pub(super) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
    action: &str,
    now: OffsetDateTime,
) -> EnrollmentResult<()> {
    sqlx::query(
        "INSERT INTO commercial_audit_event \
         (id, organization_id, actor_account_id, action, occurred_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .bind(action)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

pub(super) async fn organization_product_active(
    tx: &mut Transaction<'_, Postgres>,
    organization: OrganizationId,
    product: ProductId,
) -> EnrollmentResult<bool> {
    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT product_id FROM organization_product \
         WHERE organization_id = $1 AND product_id = $2 AND status = 'active'",
    )
    .bind(organization.as_uuid())
    .bind(product.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(active.is_some())
}

pub(super) async fn product_status(
    tx: &mut Transaction<'_, Postgres>,
    product: ProductId,
) -> EnrollmentResult<LifecycleStatus> {
    let status: String = sqlx::query_scalar("SELECT status FROM product WHERE id = $1")
        .bind(product.as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or(EnrollmentError::Invalid)?;
    status.parse().map_err(|_| EnrollmentError::Invalid)
}

pub(super) async fn active_binding(
    tx: &mut Transaction<'_, Postgres>,
    installation: InstallationId,
) -> EnrollmentResult<Option<EnrollmentCompletion>> {
    let row = sqlx::query(
        "SELECT id, organization_id, installation_id, enrollment_request_id, \
                installation_identity_id, bound_at \
         FROM installation_binding WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(row.map(|row| EnrollmentCompletion {
        enrollment_request_id: EnrollmentRequestId::from_uuid(row.get("enrollment_request_id")),
        installation_id: InstallationId::from_uuid(row.get("installation_id")),
        organization_id: OrganizationId::from_uuid(row.get("organization_id")),
        identity_id: row.get("installation_identity_id"),
        binding_id: row.get("id"),
        bound_at: row.get("bound_at"),
    }))
}
