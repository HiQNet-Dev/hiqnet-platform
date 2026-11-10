//! Casos de uso de suscripciones: creación, transiciones y renovación.

use super::pagination::*;
use super::validation::*;
use super::*;

impl<R: CommercialRepository> CommercialService<R> {
    pub async fn list_subscriptions(
        &self,
        actor: &CommercialActor,
        filters: SubscriptionFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> CommercialResult<Page<Subscription>> {
        actor.require(Capability::SubscriptionsRead)?;
        let (limit, after) = page_params(limit, cursor.as_deref(), "subscription")?;
        let mut items = self
            .repository
            .list_subscriptions(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(&mut items, limit, "subscription", |item| {
            (item.created_at, item.id)
        });
        Ok(Page { items, next_cursor })
    }

    pub async fn get_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
    ) -> CommercialResult<SubscriptionDetail> {
        actor.require(Capability::SubscriptionsRead)?;
        self.repository
            .get_subscription(id)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn create_subscription(
        &self,
        actor: &CommercialActor,
        request: CreateSubscription,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.ends_at <= request.starts_at {
            return Err(CommercialError::InvalidInput);
        }
        let command = CreateSubscriptionCommand {
            organization_id: request.organization_id,
            product_id: request.product_id,
            plan_id: request.plan_id,
            starts_at: request.starts_at,
            ends_at: request.ends_at,
            idempotency_key_hash: hash_secret(request.idempotency_key.as_bytes()).to_vec(),
            request_hash: create_request_hash(&request),
        };
        self.repository
            .create_subscription(actor.account_id, command, now)
            .await
    }

    pub async fn transition_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        action: SubscriptionAction,
        transition: SubscriptionTransition,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        if let Some(reason) = transition.reason.as_deref() {
            if reason.trim().is_empty() || reason.chars().count() > 500 {
                return Err(CommercialError::InvalidInput);
            }
        }
        self.repository
            .transition_subscription(actor.account_id, id, action, transition.reason, now)
            .await
    }

    pub async fn renew_subscription(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        request: RenewSubscription,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        actor.require(Capability::SubscriptionsManage)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.ends_at <= now {
            return Err(CommercialError::InvalidInput);
        }
        let command = RenewSubscriptionCommand {
            plan_id: request.plan_id,
            ends_at: request.ends_at,
            idempotency_key_hash: hash_secret(request.idempotency_key.as_bytes()).to_vec(),
            request_hash: renew_request_hash(&request),
        };
        self.repository
            .renew_subscription(actor.account_id, id, command, now)
            .await
    }
}
