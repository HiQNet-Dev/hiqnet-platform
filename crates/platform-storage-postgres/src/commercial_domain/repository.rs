//! Implementación del puerto `CommercialRepository`.
//!
//! Rust no permite repartir una implementación de trait entre módulos, así que
//! el bloque permanece unido; mappers, SQL y helpers viven en sus archivos.

use super::*;

#[async_trait]
impl CommercialRepository for PgCommercialDomainRepository {
    async fn list_plans(
        &self,
        filters: &PlanFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> CommercialResult<Vec<Plan>> {
        let sql = format!(
            "{PLAN_SELECT}
             WHERE ($1::uuid IS NULL OR p.product_id = $1)
               AND ($2::text IS NULL OR p.status = $2)
               AND ($3::timestamptz IS NULL OR (p.created_at, p.id) < ($3, $4::uuid))
             ORDER BY p.created_at DESC, p.id DESC
             LIMIT $5"
        );
        let rows = sqlx::query(&sql)
            .bind(filters.product_id.map(|id| id.as_uuid()))
            .bind(filters.status.map(PlanStatus::as_str))
            .bind(after.map(|cursor| cursor.timestamp))
            .bind(after.map(|cursor| cursor.id))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(plan_row).collect()
    }

    async fn get_plan(&self, id: Uuid) -> CommercialResult<Option<Plan>> {
        let sql = format!("{PLAN_SELECT} WHERE p.id = $1");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(plan_row).transpose()
    }

    async fn create_plan(
        &self,
        actor: StaffAccountId,
        plan: NewPlan,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let product_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product WHERE id = $1)")
                .bind(plan.product_id.as_uuid())
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        if !product_exists {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::NotFound);
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO plan
             (id, product_id, key, name, description, status, is_trial, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        )
        .bind(id)
        .bind(plan.product_id.as_uuid())
        .bind(&plan.key)
        .bind(&plan.name)
        .bind(&plan.description)
        .bind(plan.status.as_str())
        .bind(plan.is_trial)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(conflict_or_storage)?;
        for (key, value) in &plan.features {
            sqlx::query("INSERT INTO plan_feature (plan_id, key, value) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
        for (key, value) in &plan.limits {
            sqlx::query("INSERT INTO plan_limit (plan_id, key, value) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
        audit(
            &mut tx,
            actor,
            "plan_created",
            "plan",
            id,
            None,
            Some(plan.product_id.as_uuid()),
            now,
        )
        .await?;
        let created = load_plan(&mut tx, id)
            .await?
            .ok_or(CommercialError::Storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(created)
    }

    async fn update_plan(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        patch: PlanPatch,
        now: OffsetDateTime,
    ) -> CommercialResult<Plan> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let current = load_plan(&mut tx, id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if current.subscription_count > 0 && (patch.features.is_some() || patch.limits.is_some()) {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        sqlx::query(
            "UPDATE plan SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                updated_at = $5
             WHERE id = $1",
        )
        .bind(id)
        .bind(patch.name.as_deref())
        .bind(patch.description.as_deref())
        .bind(patch.status.map(PlanStatus::as_str))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(conflict_or_storage)?;
        if let Some(features) = patch.features {
            sqlx::query("DELETE FROM plan_feature WHERE plan_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(conflict_or_storage)?;
            for (key, value) in features {
                sqlx::query("INSERT INTO plan_feature (plan_id, key, value) VALUES ($1, $2, $3)")
                    .bind(id)
                    .bind(key)
                    .bind(value)
                    .execute(&mut *tx)
                    .await
                    .map_err(conflict_or_storage)?;
            }
        }
        if let Some(limits) = patch.limits {
            sqlx::query("DELETE FROM plan_limit WHERE plan_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(conflict_or_storage)?;
            for (key, value) in limits {
                sqlx::query("INSERT INTO plan_limit (plan_id, key, value) VALUES ($1, $2, $3)")
                    .bind(id)
                    .bind(key)
                    .bind(value)
                    .execute(&mut *tx)
                    .await
                    .map_err(conflict_or_storage)?;
            }
        }
        audit(
            &mut tx,
            actor,
            "plan_updated",
            "plan",
            id,
            None,
            Some(current.product_id.as_uuid()),
            now,
        )
        .await?;
        let updated = load_plan(&mut tx, id)
            .await?
            .ok_or(CommercialError::Storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(updated)
    }

    async fn list_subscriptions(
        &self,
        filters: &SubscriptionFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> CommercialResult<Vec<Subscription>> {
        let sql = format!(
            "{SUBSCRIPTION_SELECT}
             WHERE ($1::uuid IS NULL OR s.organization_id = $1)
               AND ($2::uuid IS NULL OR s.product_id = $2)
               AND ($3::text IS NULL OR s.status = $3)
               AND ($4::timestamptz IS NULL OR (s.created_at, s.id) < ($4, $5::uuid))
             ORDER BY s.created_at DESC, s.id DESC
             LIMIT $6"
        );
        let rows = sqlx::query(&sql)
            .bind(filters.organization_id.map(|id| id.as_uuid()))
            .bind(filters.product_id.map(|id| id.as_uuid()))
            .bind(filters.status.map(SubscriptionStatus::as_str))
            .bind(after.map(|cursor| cursor.timestamp))
            .bind(after.map(|cursor| cursor.id))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(subscription_row).collect()
    }

    async fn get_subscription(&self, id: Uuid) -> CommercialResult<Option<SubscriptionDetail>> {
        let sql = format!("{SUBSCRIPTION_SELECT} WHERE s.id = $1");
        let Some(row) = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let subscription = subscription_row(&row)?;
        let events = sqlx::query(
            "SELECT id, event, from_status, to_status, occurred_at, reason
             FROM subscription_event WHERE subscription_id = $1
             ORDER BY occurred_at DESC, id DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?
        .iter()
        .map(|event| {
            Ok(SubscriptionEvent {
                id: event.try_get("id").map_err(storage)?,
                event: event.try_get("event").map_err(storage)?,
                from_status: event.try_get("from_status").map_err(storage)?,
                to_status: event.try_get("to_status").map_err(storage)?,
                occurred_at: event.try_get("occurred_at").map_err(storage)?,
                reason: event.try_get("reason").map_err(storage)?,
            })
        })
        .collect::<CommercialResult<Vec<_>>>()?;
        Ok(Some(SubscriptionDetail {
            subscription,
            events,
        }))
    }

    async fn create_subscription(
        &self,
        actor: StaffAccountId,
        command: CreateSubscriptionCommand,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if let Some(existing) = idempotent_subscription(
            &mut tx,
            "subscription_create",
            &command.idempotency_key_hash,
            &command.request_hash,
        )
        .await?
        {
            tx.commit().await.map_err(storage)?;
            return Ok(existing);
        }
        let org_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM organization WHERE id = $1")
                .bind(command.organization_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        let Some(org_status) = org_status else {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::NotFound);
        };
        if org_status != "active" {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        let org_product: Option<String> = sqlx::query_scalar(
            "SELECT status FROM organization_product
             WHERE organization_id = $1 AND product_id = $2 FOR UPDATE",
        )
        .bind(command.organization_id.as_uuid())
        .bind(command.product_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if org_product.as_deref() != Some("active") {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        let plan = load_plan(&mut tx, command.plan_id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if plan.product_id != command.product_id || plan.status != PlanStatus::Active {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO subscription
             (id, organization_id, product_id, plan_id, status, is_trial, starts_at, ends_at,
              created_at, updated_at, idempotency_scope, idempotency_key_hash, request_hash)
             VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, $8, $8,
                     'subscription_create', $9, $10)",
        )
        .bind(id)
        .bind(command.organization_id.as_uuid())
        .bind(command.product_id.as_uuid())
        .bind(command.plan_id)
        .bind(plan.is_trial)
        .bind(command.starts_at)
        .bind(command.ends_at)
        .bind(now)
        .bind(&command.idempotency_key_hash)
        .bind(&command.request_hash)
        .execute(&mut *tx)
        .await
        .map_err(conflict_or_storage)?;
        event(
            &mut tx,
            id,
            "created",
            None,
            Some("active"),
            now,
            actor,
            None,
        )
        .await?;
        audit(
            &mut tx,
            actor,
            "subscription_created",
            "subscription",
            id,
            Some(command.organization_id.as_uuid()),
            Some(command.product_id.as_uuid()),
            now,
        )
        .await?;
        let created = load_subscription(&mut tx, id)
            .await?
            .ok_or(CommercialError::Storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(created)
    }

    async fn transition_subscription(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        action: SubscriptionAction,
        reason: Option<String>,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let current = load_subscription(&mut tx, id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        let (target, event_name) = match action {
            SubscriptionAction::Suspend if current.status == SubscriptionStatus::Active => {
                (SubscriptionStatus::Suspended, "suspended")
            }
            SubscriptionAction::Reactivate if current.status == SubscriptionStatus::Suspended => {
                (SubscriptionStatus::Active, "reactivated")
            }
            SubscriptionAction::Cancel
                if matches!(
                    current.status,
                    SubscriptionStatus::Active | SubscriptionStatus::Suspended
                ) =>
            {
                (SubscriptionStatus::Cancelled, "cancelled")
            }
            _ => {
                tx.rollback().await.map_err(storage)?;
                return Err(CommercialError::Conflict);
            }
        };
        sqlx::query(
            "UPDATE subscription SET status = $2, updated_at = $3,
                suspended_at = CASE WHEN $2 = 'suspended' THEN $3 ELSE NULL END,
                cancelled_at = CASE WHEN $2 = 'cancelled' THEN $3 ELSE cancelled_at END
             WHERE id = $1",
        )
        .bind(id)
        .bind(target.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(conflict_or_storage)?;
        event(
            &mut tx,
            id,
            event_name,
            Some(current.status.as_str()),
            Some(target.as_str()),
            now,
            actor,
            reason.as_deref(),
        )
        .await?;
        audit(
            &mut tx,
            actor,
            action.audit_action(),
            "subscription",
            id,
            Some(current.organization_id.as_uuid()),
            Some(current.product_id.as_uuid()),
            now,
        )
        .await?;
        let updated = load_subscription(&mut tx, id)
            .await?
            .ok_or(CommercialError::Storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(updated)
    }

    async fn renew_subscription(
        &self,
        actor: StaffAccountId,
        id: Uuid,
        command: RenewSubscriptionCommand,
        now: OffsetDateTime,
    ) -> CommercialResult<Subscription> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if let Some(existing) = idempotent_subscription(
            &mut tx,
            "subscription_renew",
            &command.idempotency_key_hash,
            &command.request_hash,
        )
        .await?
        {
            tx.commit().await.map_err(storage)?;
            return Ok(existing);
        }
        let current = load_subscription(&mut tx, id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if !matches!(
            current.status,
            SubscriptionStatus::Active | SubscriptionStatus::Expired
        ) {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        let plan_id = command.plan_id.unwrap_or(current.plan_id);
        let plan = load_plan(&mut tx, plan_id)
            .await?
            .ok_or(CommercialError::NotFound)?;
        if plan.product_id != current.product_id
            || plan.status != PlanStatus::Active
            || plan.is_trial
        {
            tx.rollback().await.map_err(storage)?;
            return Err(CommercialError::Conflict);
        }
        if current.status == SubscriptionStatus::Active {
            sqlx::query(
                "UPDATE subscription SET status = 'cancelled', cancelled_at = $2, updated_at = $2
                 WHERE id = $1",
            )
            .bind(current.id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            event(
                &mut tx,
                current.id,
                "cancelled",
                Some("active"),
                Some("cancelled"),
                now,
                actor,
                Some("renewed"),
            )
            .await?;
        }
        let new_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO subscription
             (id, organization_id, product_id, plan_id, status, is_trial, starts_at, ends_at,
              created_at, updated_at, renewed_from_subscription_id,
              idempotency_scope, idempotency_key_hash, request_hash)
             VALUES ($1, $2, $3, $4, 'active', false, $5, $6, $7, $7, $8,
                     'subscription_renew', $9, $10)",
        )
        .bind(new_id)
        .bind(current.organization_id.as_uuid())
        .bind(current.product_id.as_uuid())
        .bind(plan_id)
        .bind(now)
        .bind(command.ends_at)
        .bind(now)
        .bind(current.id)
        .bind(&command.idempotency_key_hash)
        .bind(&command.request_hash)
        .execute(&mut *tx)
        .await
        .map_err(conflict_or_storage)?;
        event(
            &mut tx,
            new_id,
            "renewed",
            None,
            Some("active"),
            now,
            actor,
            None,
        )
        .await?;
        audit(
            &mut tx,
            actor,
            "subscription_renewed",
            "subscription",
            new_id,
            Some(current.organization_id.as_uuid()),
            Some(current.product_id.as_uuid()),
            now,
        )
        .await?;
        let created = load_subscription(&mut tx, new_id)
            .await?
            .ok_or(CommercialError::Storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(created)
    }

    async fn entitlements(
        &self,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<Entitlements>> {
        let org_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM organization WHERE id = $1")
                .bind(organization_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        let Some(org_status) = org_status else {
            return Ok(None);
        };
        if org_status != "active" {
            return Ok(Some(Entitlements {
                effective: false,
                status: None,
                reason: "organization_inactive".into(),
                plan: None,
                subscription_id: None,
                is_trial: false,
                features: BTreeMap::new(),
                limits: BTreeMap::new(),
                starts_at: None,
                ends_at: None,
            }));
        }
        let association: Option<String> = sqlx::query_scalar(
            "SELECT status FROM organization_product
             WHERE organization_id = $1 AND product_id = $2",
        )
        .bind(organization_id.as_uuid())
        .bind(product_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        if association.as_deref() != Some("active") {
            return Ok(Some(Entitlements {
                effective: false,
                status: None,
                reason: "product_not_enabled".into(),
                plan: None,
                subscription_id: None,
                is_trial: false,
                features: BTreeMap::new(),
                limits: BTreeMap::new(),
                starts_at: None,
                ends_at: None,
            }));
        }
        let sql = format!(
            "{SUBSCRIPTION_SELECT}
             WHERE s.organization_id = $1 AND s.product_id = $2
             ORDER BY s.created_at DESC, s.id DESC
             LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(organization_id.as_uuid())
            .bind(product_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        let Some(row) = row else {
            return Ok(Some(Entitlements {
                effective: false,
                status: None,
                reason: "no_subscription".into(),
                plan: None,
                subscription_id: None,
                is_trial: false,
                features: BTreeMap::new(),
                limits: BTreeMap::new(),
                starts_at: None,
                ends_at: None,
            }));
        };
        let subscription = subscription_row(&row)?;
        let plan = sqlx::query(&format!("{PLAN_SELECT} WHERE p.id = $1"))
            .bind(subscription.plan_id)
            .fetch_one(&self.pool)
            .await
            .map_err(storage)
            .and_then(|row| plan_row(&row))?;
        let mut result = Entitlements {
            effective: false,
            status: Some(subscription.status),
            reason: "unknown".into(),
            plan: Some(subscription.plan.clone()),
            subscription_id: Some(subscription.id),
            is_trial: subscription.is_trial,
            features: BTreeMap::new(),
            limits: BTreeMap::new(),
            starts_at: Some(subscription.starts_at),
            ends_at: Some(subscription.ends_at),
        };
        if matches!(
            subscription.status,
            SubscriptionStatus::Cancelled | SubscriptionStatus::Expired
        ) {
            result.reason = "terminated".into();
        } else if now >= subscription.ends_at {
            result.reason = "expired".into();
        } else if now < subscription.starts_at {
            result.reason = "not_started".into();
        } else if subscription.status == SubscriptionStatus::Suspended {
            result.reason = "suspended".into();
            result.features = plan.features;
            result.limits = plan.limits;
        } else {
            result.effective = true;
            result.reason = "active".into();
            result.features = plan.features;
            result.limits = plan.limits;
        }
        Ok(Some(result))
    }

    async fn portal_products(
        &self,
        organization_id: OrganizationId,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<Vec<PortalProduct>>> {
        if !self.portal_membership(organization_id, account_id).await? {
            return Ok(None);
        }
        let rows = sqlx::query(
            "SELECT p.id, p.key, p.name
             FROM organization_product op
             JOIN product p ON p.id = op.product_id
             WHERE op.organization_id = $1 AND op.status = 'active'
             ORDER BY p.name, p.key",
        )
        .bind(organization_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut products = Vec::with_capacity(rows.len());
        for row in rows {
            let product_id = ProductId::from_uuid(row.try_get("id").map_err(storage)?);
            let entitlements = self
                .entitlements(organization_id, product_id, now)
                .await?
                .ok_or(CommercialError::Storage)?;
            products.push(PortalProduct {
                product: PortalProductRef {
                    id: product_id,
                    key: row.try_get("key").map_err(storage)?,
                    name: row.try_get("name").map_err(storage)?,
                },
                organization_product_status: "active".into(),
                subscription: portal_subscription(&entitlements),
                entitlements,
            });
        }
        Ok(Some(products))
    }

    async fn portal_product(
        &self,
        organization_id: OrganizationId,
        product_id: ProductId,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<Option<PortalProduct>> {
        if !self.portal_membership(organization_id, account_id).await? {
            return Ok(None);
        }
        let product = sqlx::query(
            "SELECT p.id, p.key, p.name
             FROM organization_product op
             JOIN product p ON p.id = op.product_id
             WHERE op.organization_id = $1 AND op.product_id = $2 AND op.status = 'active'",
        )
        .bind(organization_id.as_uuid())
        .bind(product_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(product) = product else {
            return Ok(None);
        };
        let entitlements = self
            .entitlements(organization_id, product_id, now)
            .await?
            .ok_or(CommercialError::Storage)?;
        Ok(Some(PortalProduct {
            product: PortalProductRef {
                id: product_id,
                key: product.try_get("key").map_err(storage)?,
                name: product.try_get("name").map_err(storage)?,
            },
            organization_product_status: "active".into(),
            subscription: portal_subscription(&entitlements),
            entitlements,
        }))
    }
}
