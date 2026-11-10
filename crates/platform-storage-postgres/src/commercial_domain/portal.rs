//! Membresía del Portal y proyección de suscripción.

use super::*;

impl PgCommercialDomainRepository {
    pub(super) async fn portal_membership(
        &self,
        organization_id: OrganizationId,
        account_id: CommercialAccountId,
    ) -> CommercialResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM organization_membership
                WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL
             )",
        )
        .bind(organization_id.as_uuid())
        .bind(account_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage)
    }
}

pub(super) fn portal_subscription(entitlements: &Entitlements) -> Option<PortalSubscription> {
    let (Some(id), Some(status), Some(plan), Some(starts_at), Some(ends_at)) = (
        entitlements.subscription_id,
        entitlements.status,
        entitlements.plan.clone(),
        entitlements.starts_at,
        entitlements.ends_at,
    ) else {
        return None;
    };
    Some(PortalSubscription {
        id,
        status,
        plan,
        starts_at,
        ends_at,
        is_trial: entitlements.is_trial,
    })
}
