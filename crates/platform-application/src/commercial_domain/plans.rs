//! Casos de uso de planes comerciales.

use super::pagination::*;
use super::validation::*;
use super::*;

impl<R: CommercialRepository> CommercialService<R> {
    pub async fn list_plans(
        &self,
        actor: &CommercialActor,
        filters: PlanFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> CommercialResult<Page<Plan>> {
        actor.require(Capability::ProductsRead)?;
        let (limit, after) = page_params(limit, cursor.as_deref(), "plan")?;
        let mut items = self
            .repository
            .list_plans(&filters, limit + 1, after)
            .await?;
        let next_cursor =
            take_page_cursor(&mut items, limit, "plan", |item| (item.created_at, item.id));
        Ok(Page { items, next_cursor })
    }

    pub async fn get_plan(&self, actor: &CommercialActor, id: Uuid) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsRead)?;
        self.repository
            .get_plan(id)
            .await?
            .ok_or(CommercialError::NotFound)
    }

    pub async fn create_plan(
        &self,
        actor: &CommercialActor,
        plan: NewPlan,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsManage)?;
        validate_plan(&plan)?;
        self.repository
            .create_plan(actor.account_id, plan, now)
            .await
    }

    pub async fn update_plan(
        &self,
        actor: &CommercialActor,
        id: Uuid,
        patch: PlanPatch,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        actor.require(Capability::ProductsManage)?;
        if let Some(name) = patch.name.as_deref() {
            validate_name(name)?;
        }
        if let Some(features) = &patch.features {
            validate_features(features)?;
        }
        if let Some(limits) = &patch.limits {
            validate_limits(limits)?;
        }
        let current = self
            .repository
            .get_plan(id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if let Some(status) = patch.status {
            if !current.status.can_transition_to(status) {
                return Err(CommercialError::Conflict);
            }
        }
        if current.subscription_count > 0 && (patch.features.is_some() || patch.limits.is_some()) {
            return Err(CommercialError::Conflict);
        }
        self.repository
            .update_plan(actor.account_id, id, patch, now)
            .await
    }
}
