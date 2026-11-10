use platform_application::{
    control::AssociationStatus,
    organization_product::{OrganizationProductRepository, OrganizationProductService},
};
use platform_core::ids::{OrganizationId, ProductId, StaffAccountId};
use platform_storage_postgres::{PgOrganizationProductRepository, open};
use sqlx::PgPool;
use time::OffsetDateTime;
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
         VALUES ($1, $2, 'Platform Admin', 'active', $3, $3)",
    )
    .bind(staff)
    .bind(format!("op-{staff}@hiqnet.test"))
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
    .bind(format!("OP org {organization}"))
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
    .bind(format!("op-{}", &product.to_string()[..8]))
    .bind(format!("OP product {product}"))
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

#[tokio::test]
async fn activate_and_deactivate_are_idempotent_and_audited() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let repo = PgOrganizationProductRepository::new(pool.clone());
    let actor = StaffAccountId::from_uuid(fixture.staff);
    let organization = OrganizationId::from_uuid(fixture.organization);
    let product = ProductId::from_uuid(fixture.product);

    let first = repo
        .activate(actor, organization, product, now)
        .await
        .unwrap();
    assert!(first.changed);
    assert_eq!(first.view.association_status, AssociationStatus::Active);
    let activated_at = first.view.activated_at;

    let second = repo
        .activate(
            actor,
            organization,
            product,
            now + time::Duration::seconds(1),
        )
        .await
        .unwrap();
    assert!(!second.changed);
    assert_eq!(second.view.activated_at, activated_at);

    let inactive = repo
        .set_status(
            actor,
            organization,
            product,
            AssociationStatus::Inactive,
            now + time::Duration::seconds(2),
        )
        .await
        .unwrap();
    assert!(inactive.changed);
    assert_eq!(
        inactive.view.association_status,
        AssociationStatus::Inactive
    );
    assert_eq!(inactive.view.activated_at, activated_at);

    let inactive_again = repo
        .set_status(
            actor,
            organization,
            product,
            AssociationStatus::Inactive,
            now + time::Duration::seconds(3),
        )
        .await
        .unwrap();
    assert!(!inactive_again.changed);

    let active_again = repo
        .set_status(
            actor,
            organization,
            product,
            AssociationStatus::Active,
            now + time::Duration::seconds(4),
        )
        .await
        .unwrap();
    assert!(active_again.changed);
    assert_eq!(
        active_again.view.association_status,
        AssociationStatus::Active
    );
    assert_eq!(active_again.view.activated_at, activated_at);

    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM staff_audit_event
         WHERE organization_id = $1 AND product_id = $2
         ORDER BY occurred_at",
    )
    .bind(fixture.organization)
    .bind(fixture.product)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actions,
        vec![
            "organization_product_activated",
            "organization_product_deactivated",
            "organization_product_activated",
        ]
    );

    assert!(
        sqlx::query(
            "DELETE FROM organization_product WHERE organization_id = $1 AND product_id = $2"
        )
        .bind(fixture.organization)
        .bind(fixture.product)
        .execute(&pool)
        .await
        .is_err()
    );
}

#[tokio::test]
async fn missing_resources_are_not_found_and_read_is_available() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let repo = PgOrganizationProductRepository::new(pool.clone());
    let actor = StaffAccountId::from_uuid(fixture.staff);
    let organization = OrganizationId::from_uuid(fixture.organization);
    let product = ProductId::from_uuid(fixture.product);

    let empty = repo.list(organization).await.unwrap();
    assert!(empty.is_empty());
    assert_eq!(
        repo.activate(actor, OrganizationId::new(), product, now)
            .await,
        Err(platform_application::organization_product::OrganizationProductError::NotFound)
    );
    assert_eq!(
        repo.activate(actor, organization, ProductId::new(), now)
            .await,
        Err(platform_application::organization_product::OrganizationProductError::NotFound)
    );
    assert_eq!(
        repo.set_status(actor, organization, product, AssociationStatus::Active, now)
            .await,
        Err(platform_application::organization_product::OrganizationProductError::NotFound)
    );

    repo.activate(actor, organization, product, now)
        .await
        .unwrap();
    let listed = repo.list(organization).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].product_id, product);
}

#[tokio::test]
async fn service_requires_manage_for_mutations() {
    use platform_application::organization_product::{
        OrganizationProductActor, OrganizationProductError,
    };
    use platform_core::staff::StaffRole;

    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let service =
        OrganizationProductService::new(PgOrganizationProductRepository::new(pool.clone()));
    let reader = OrganizationProductActor::new(
        StaffAccountId::from_uuid(fixture.staff),
        StaffRole::ReadOnly,
    );
    assert_eq!(
        service
            .activate(
                &reader,
                OrganizationId::from_uuid(fixture.organization),
                ProductId::from_uuid(fixture.product),
                now,
            )
            .await,
        Err(OrganizationProductError::Forbidden)
    );
    assert_eq!(
        service
            .list(&reader, OrganizationId::from_uuid(fixture.organization),)
            .await
            .unwrap()
            .len(),
        0
    );
}
