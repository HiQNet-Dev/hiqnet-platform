//! Conversión de filas SQL a modelos de dominio.

use super::*;

pub(super) fn storage(_error: sqlx::Error) -> CommercialError {
    CommercialError::Storage
}

pub(super) fn conflict_or_storage(error: sqlx::Error) -> CommercialError {
    if let sqlx::Error::Database(database) = &error {
        if database.code().as_deref() == Some("23505") {
            return CommercialError::Conflict;
        }
        if database.code().as_deref() == Some("23503") {
            return CommercialError::NotFound;
        }
    }
    CommercialError::Storage
}

pub(super) fn plan_status(value: &str) -> CommercialResult<PlanStatus> {
    PlanStatus::from_str(value).map_err(|_| CommercialError::Storage)
}

pub(super) fn subscription_status(value: &str) -> CommercialResult<SubscriptionStatus> {
    SubscriptionStatus::from_str(value).map_err(|_| CommercialError::Storage)
}

pub(super) fn plan_ref(row: &PgRow) -> CommercialResult<PlanRef> {
    Ok(PlanRef {
        id: row.try_get("plan_id").map_err(storage)?,
        key: row.try_get("plan_key").map_err(storage)?,
        name: row.try_get("plan_name").map_err(storage)?,
        is_trial: row.try_get("plan_is_trial").map_err(storage)?,
    })
}

pub(super) fn plan_row(row: &PgRow) -> CommercialResult<Plan> {
    let status: String = row.try_get("status").map_err(storage)?;
    let features: serde_json::Value = row.try_get("features").map_err(storage)?;
    let limits: serde_json::Value = row.try_get("limits").map_err(storage)?;
    Ok(Plan {
        id: row.try_get("id").map_err(storage)?,
        product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage)?),
        product_key: row.try_get("product_key").map_err(storage)?,
        product_name: row.try_get("product_name").map_err(storage)?,
        key: row.try_get("key").map_err(storage)?,
        name: row.try_get("name").map_err(storage)?,
        description: row.try_get("description").map_err(storage)?,
        status: plan_status(&status)?,
        is_trial: row.try_get("is_trial").map_err(storage)?,
        features: serde_json::from_value(features).map_err(|_| CommercialError::Storage)?,
        limits: serde_json::from_value(limits).map_err(|_| CommercialError::Storage)?,
        subscription_count: row.try_get("subscription_count").map_err(storage)?,
        created_at: row.try_get("created_at").map_err(storage)?,
        updated_at: row.try_get("updated_at").map_err(storage)?,
    })
}

pub(super) fn subscription_row(row: &PgRow) -> CommercialResult<Subscription> {
    let status: String = row.try_get("status").map_err(storage)?;
    Ok(Subscription {
        id: row.try_get("id").map_err(storage)?,
        organization_id: OrganizationId::from_uuid(
            row.try_get("organization_id").map_err(storage)?,
        ),
        product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage)?),
        plan_id: row.try_get("plan_id").map_err(storage)?,
        plan: plan_ref(row)?,
        status: subscription_status(&status)?,
        is_trial: row.try_get("is_trial").map_err(storage)?,
        starts_at: row.try_get("starts_at").map_err(storage)?,
        ends_at: row.try_get("ends_at").map_err(storage)?,
        created_at: row.try_get("created_at").map_err(storage)?,
        updated_at: row.try_get("updated_at").map_err(storage)?,
        suspended_at: row.try_get("suspended_at").map_err(storage)?,
        cancelled_at: row.try_get("cancelled_at").map_err(storage)?,
        renewed_from_subscription_id: row
            .try_get("renewed_from_subscription_id")
            .map_err(storage)?,
    })
}
