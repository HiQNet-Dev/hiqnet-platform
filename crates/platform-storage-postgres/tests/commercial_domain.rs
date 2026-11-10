use platform_application::commercial_domain::{
    CommercialActor, CommercialError, CommercialService, CreateSubscription, NewPlan, PlanFilters,
    PlanPatch, PlanStatus, RenewSubscription, SubscriptionAction, SubscriptionFilters,
    SubscriptionStatus, SubscriptionTransition,
};
use platform_core::{
    ids::{OrganizationId, ProductId, StaffAccountId},
    staff::StaffRole,
};
use platform_storage_postgres::{PgCommercialDomainRepository, open};
use sqlx::PgPool;
use std::collections::BTreeMap;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn test_url() -> String {
    let url = std::env::var("TEST_DATABASE_URL").expect("base aislada requerida");
    assert!(
        url.rsplit('/')
            .next()
            .is_some_and(|database| database.contains("test") && database != "hiqnet"),
        "TEST_DATABASE_URL debe apuntar a una base aislada"
    );
    url
}

struct Fixture {
    staff: Uuid,
    organization: Uuid,
    product: Uuid,
}

async fn seed(pool: &PgPool, now: OffsetDateTime) -> Fixture {
    let staff = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_account (id, email, display_name, status, created_at, updated_at)
         VALUES ($1, $2, 'Commercial Admin', 'active', $3, $3)",
    )
    .bind(staff)
    .bind(format!("commercial-{staff}@hiqnet.test"))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let organization = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
         VALUES ($1, $2, 'active', $3)",
    )
    .bind(organization)
    .bind(format!("Commercial org {organization}"))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let product = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
         VALUES ($1, $2, $3, '', 'active', 'saas', $4, $4)",
    )
    .bind(product)
    .bind(format!("commercial-{}", &product.to_string()[..8]))
    .bind(format!("Commercial product {product}"))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
         VALUES ($1, $2, 'active', $3)",
    )
    .bind(organization)
    .bind(product)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    Fixture {
        staff,
        organization,
        product,
    }
}

fn plan(fixture: &Fixture, key: &str, is_trial: bool, seats: i64) -> NewPlan {
    NewPlan {
        product_id: ProductId::from_uuid(fixture.product),
        key: key.into(),
        name: format!("Plan {key}"),
        description: String::new(),
        status: PlanStatus::Active,
        is_trial,
        features: BTreeMap::from([("sync".into(), true)]),
        limits: BTreeMap::from([("seats".into(), seats)]),
    }
}

#[tokio::test]
async fn subscription_lifecycle_idempotency_trial_and_entitlements() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let service = CommercialService::new(PgCommercialDomainRepository::new(pool.clone()));
    let actor = CommercialActor::new(
        StaffAccountId::from_uuid(fixture.staff),
        StaffRole::PlatformAdmin,
    );
    let trial = service
        .create_plan(&actor, plan(&fixture, "trial", true, 1), now)
        .await
        .unwrap();
    let _paid = service
        .create_plan(&actor, plan(&fixture, "pro", false, 10), now)
        .await
        .unwrap();
    assert_eq!(
        service
            .list_plans(
                &actor,
                PlanFilters {
                    product_id: Some(ProductId::from_uuid(fixture.product)),
                    status: Some(PlanStatus::Active),
                },
                Some(10),
                None,
            )
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    let trial_key = format!("trial-{}", Uuid::new_v4());
    let request = CreateSubscription {
        organization_id: OrganizationId::from_uuid(fixture.organization),
        product_id: ProductId::from_uuid(fixture.product),
        plan_id: trial.id,
        starts_at: now,
        ends_at: now + Duration::days(7),
        idempotency_key: trial_key,
    };
    let created = service
        .create_subscription(&actor, request.clone(), now)
        .await
        .unwrap();
    let repeated = service
        .create_subscription(&actor, request.clone(), now + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(created.id, repeated.id);
    let mut changed = request;
    changed.ends_at = now + Duration::days(8);
    assert_eq!(
        service.create_subscription(&actor, changed, now).await,
        Err(CommercialError::IdempotencyConflict)
    );

    let entitlements = service
        .entitlements(
            &actor,
            OrganizationId::from_uuid(fixture.organization),
            ProductId::from_uuid(fixture.product),
            now,
        )
        .await
        .unwrap();
    assert!(entitlements.effective);
    assert_eq!(entitlements.features.get("sync"), Some(&true));

    let suspended = service
        .transition_subscription(
            &actor,
            created.id,
            SubscriptionAction::Suspend,
            SubscriptionTransition {
                reason: Some("review".into()),
            },
            now + Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(suspended.status, SubscriptionStatus::Suspended);
    let suspended_entitlements = service
        .entitlements(
            &actor,
            OrganizationId::from_uuid(fixture.organization),
            ProductId::from_uuid(fixture.product),
            now + Duration::hours(2),
        )
        .await
        .unwrap();
    assert!(!suspended_entitlements.effective);
    assert_eq!(suspended_entitlements.reason, "suspended");
    assert_eq!(suspended_entitlements.features.get("sync"), Some(&true));

    let reactivated = service
        .transition_subscription(
            &actor,
            created.id,
            SubscriptionAction::Reactivate,
            SubscriptionTransition { reason: None },
            now + Duration::hours(3),
        )
        .await
        .unwrap();
    assert_eq!(reactivated.status, SubscriptionStatus::Active);

    let new_plan = service
        .create_plan(&actor, plan(&fixture, "pro-renew", false, 20), now)
        .await
        .unwrap();
    let renewed = service
        .renew_subscription(
            &actor,
            created.id,
            RenewSubscription {
                plan_id: Some(new_plan.id),
                ends_at: now + Duration::days(60),
                idempotency_key: format!("renew-{}", Uuid::new_v4()),
            },
            now + Duration::hours(4),
        )
        .await
        .unwrap();
    assert_eq!(renewed.status, SubscriptionStatus::Active);
    assert_eq!(renewed.renewed_from_subscription_id, Some(created.id));
    assert_eq!(renewed.plan_id, new_plan.id);
    let old = service.get_subscription(&actor, created.id).await.unwrap();
    assert_eq!(old.subscription.status, SubscriptionStatus::Cancelled);
    assert!(
        old.events
            .iter()
            .any(|event| event.event == "cancelled" && event.reason.as_deref() == Some("renewed"))
    );
    let renewed_detail = service.get_subscription(&actor, renewed.id).await.unwrap();
    assert!(
        renewed_detail
            .events
            .iter()
            .any(|event| event.event == "renewed")
    );

    let current = service
        .list_subscriptions(
            &actor,
            SubscriptionFilters {
                organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
                ..SubscriptionFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(current.items.len(), 2);

    let old_event = old.events[0].id;
    assert!(
        sqlx::query("UPDATE subscription_event SET reason = 'x' WHERE id = $1")
            .bind(old_event)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM subscription_event WHERE id = $1")
            .bind(old_event)
            .execute(&pool)
            .await
            .is_err()
    );

    let cancelled = service
        .transition_subscription(
            &actor,
            renewed.id,
            SubscriptionAction::Cancel,
            SubscriptionTransition {
                reason: Some("end".into()),
            },
            now + Duration::hours(5),
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status, SubscriptionStatus::Cancelled);
    let terminated = service
        .entitlements(
            &actor,
            OrganizationId::from_uuid(fixture.organization),
            ProductId::from_uuid(fixture.product),
            now + Duration::hours(6),
        )
        .await
        .unwrap();
    assert!(!terminated.effective);
    assert_eq!(terminated.reason, "terminated");

    assert_eq!(
        service
            .create_subscription(
                &actor,
                CreateSubscription {
                    organization_id: OrganizationId::from_uuid(fixture.organization),
                    product_id: ProductId::from_uuid(fixture.product),
                    plan_id: trial.id,
                    starts_at: now + Duration::days(1),
                    ends_at: now + Duration::days(8),
                    idempotency_key: format!("second-trial-{}", Uuid::new_v4()),
                },
                now,
            )
            .await,
        Err(CommercialError::Conflict)
    );
    assert_eq!(
        service
            .update_plan(
                &actor,
                new_plan.id,
                PlanPatch {
                    name: None,
                    description: None,
                    status: None,
                    features: Some(BTreeMap::from([("sync".into(), false)])),
                    limits: None,
                },
                now,
            )
            .await,
        Err(CommercialError::Conflict)
    );
}
