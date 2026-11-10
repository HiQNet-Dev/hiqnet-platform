//! Auditoría, eventos e idempotencia compartidos.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: StaffAccountId,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    organization_id: Option<Uuid>,
    product_id: Option<Uuid>,
    now: OffsetDateTime,
) -> CommercialResult<()> {
    sqlx::query(
        "INSERT INTO staff_audit_event
         (id, staff_account_id, action, resource_type, resource_id, organization_id, product_id,
          result, occurred_at, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'success', $8, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(actor.as_uuid())
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(organization_id)
    .bind(product_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn event(
    tx: &mut Transaction<'_, Postgres>,
    subscription_id: Uuid,
    event: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    occurred_at: OffsetDateTime,
    actor: StaffAccountId,
    reason: Option<&str>,
) -> CommercialResult<()> {
    sqlx::query(
        "INSERT INTO subscription_event
         (id, subscription_id, event, from_status, to_status, occurred_at,
          actor_staff_account_id, reason, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(subscription_id)
    .bind(event)
    .bind(from_status)
    .bind(to_status)
    .bind(occurred_at)
    .bind(actor.as_uuid())
    .bind(reason)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

pub(super) async fn load_plan(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> CommercialResult<Option<Plan>> {
    let sql = format!("{PLAN_SELECT} WHERE p.id = $1");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?;
    row.as_ref().map(plan_row).transpose()
}

pub(super) async fn load_subscription(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> CommercialResult<Option<Subscription>> {
    let sql = format!("{SUBSCRIPTION_SELECT} WHERE s.id = $1");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?;
    row.as_ref().map(subscription_row).transpose()
}

pub(super) async fn idempotent_subscription(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    key_hash: &[u8],
    request_hash: &[u8],
) -> CommercialResult<Option<Subscription>> {
    let row = sqlx::query(
        "SELECT id, request_hash FROM subscription
         WHERE idempotency_scope = $1 AND idempotency_key_hash = $2",
    )
    .bind(scope)
    .bind(key_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: Uuid = row.try_get("id").map_err(storage)?;
    let stored_hash: Option<Vec<u8>> = row.try_get("request_hash").map_err(storage)?;
    if stored_hash.as_deref() != Some(request_hash) {
        return Err(CommercialError::IdempotencyConflict);
    }
    load_subscription(tx, id)
        .await?
        .map(Some)
        .ok_or(CommercialError::Storage)
}
