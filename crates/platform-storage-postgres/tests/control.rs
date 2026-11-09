use platform_application::control::{
    AccountStatus, AssociationStatus, ControlCursor, ControlRepository, OrganizationStatus,
};
use platform_core::ids::{CommercialAccountId, OrganizationId};
use platform_storage_postgres::{PgControlRepository, open};
use sqlx::PgPool;
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

async fn insert_organization(
    pool: &PgPool,
    name: &str,
    status: &str,
    created_at: OffsetDateTime,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(name)
    .bind(status)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_account(
    pool: &PgPool,
    email: &str,
    name: &str,
    status: &str,
    created_at: OffsetDateTime,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, status, created_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(email)
    .bind(name)
    .bind(status)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_membership(
    pool: &PgPool,
    organization_id: Uuid,
    account_id: Uuid,
    role: &str,
    joined_at: OffsetDateTime,
    revoked_at: Option<OffsetDateTime>,
) {
    sqlx::query(
        "INSERT INTO organization_membership
         (organization_id, account_id, role, role_version, joined_at, revoked_at)
         VALUES ($1, $2, $3, 1, $4, $5)",
    )
    .bind(organization_id)
    .bind(account_id)
    .bind(role)
    .bind(joined_at)
    .bind(revoked_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_product(pool: &PgPool, status: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let key = format!("product-{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
         VALUES ($1, $2, $3, '', $4, 'saas', now(), now())",
    )
    .bind(id)
    .bind(&key)
    .bind(format!("Product {key}"))
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    (id, key)
}

async fn insert_organization_product(
    pool: &PgPool,
    organization_id: Uuid,
    product_id: Uuid,
    status: &str,
    activated_at: OffsetDateTime,
) {
    sqlx::query(
        "INSERT INTO organization_product
         (organization_id, product_id, status, activated_at, entitlements)
         VALUES ($1, $2, $3, $4, '{}'::jsonb)",
    )
    .bind(organization_id)
    .bind(product_id)
    .bind(status)
    .bind(activated_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn organization_product_respeta_los_invariantes_ratificados() {
    let pool = open(&test_url()).await.unwrap();
    let organization_id =
        insert_organization(&pool, "C4 invariants", "active", OffsetDateTime::now_utc()).await;
    let other_organization =
        insert_organization(&pool, "C4 other", "archived", OffsetDateTime::now_utc()).await;
    let (product_id, _) = insert_product(&pool, "active").await;
    let (other_product, _) = insert_product(&pool, "retired").await;
    let activated_at = OffsetDateTime::now_utc();

    insert_organization_product(&pool, organization_id, product_id, "active", activated_at).await;

    let missing_organization = sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
         VALUES ($1, $2, 'active', now())",
    )
    .bind(Uuid::new_v4())
    .bind(product_id)
    .execute(&pool)
    .await;
    assert!(
        missing_organization.is_err(),
        "FK de organización inexistente"
    );

    let missing_product = sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
         VALUES ($1, $2, 'active', now())",
    )
    .bind(organization_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await;
    assert!(missing_product.is_err(), "FK de producto inexistente");

    for invalid_status in ["suspended", "trial", "expired", "past_due"] {
        let invalid = sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, $3, now())",
        )
        .bind(other_organization)
        .bind(other_product)
        .bind(invalid_status)
        .execute(&pool)
        .await;
        assert!(invalid.is_err(), "estado inválido {invalid_status}");
    }

    for invalid_entitlements in ["'[]'::jsonb", "'text'::jsonb", "'null'::jsonb"] {
        let sql = format!(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at, entitlements)
             VALUES ($1, $2, 'active', now(), {invalid_entitlements})"
        );
        let invalid = sqlx::query(&sql)
            .bind(other_organization)
            .bind(other_product)
            .execute(&pool)
            .await;
        assert!(
            invalid.is_err(),
            "entitlements inválidos {invalid_entitlements}"
        );
    }

    let identity_change = sqlx::query(
        "UPDATE organization_product SET organization_id = $3
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .bind(other_organization)
    .execute(&pool)
    .await;
    assert!(identity_change.is_err(), "organization_id inmutable");

    let product_change = sqlx::query(
        "UPDATE organization_product SET product_id = $3
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .bind(other_product)
    .execute(&pool)
    .await;
    assert!(product_change.is_err(), "product_id inmutable");

    let activated_change = sqlx::query(
        "UPDATE organization_product SET activated_at = $3
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .bind(activated_at + Duration::seconds(60))
    .execute(&pool)
    .await;
    assert!(activated_change.is_err(), "activated_at inmutable");

    let delete = sqlx::query(
        "DELETE FROM organization_product WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .execute(&pool)
    .await;
    assert!(delete.is_err(), "DELETE prohibido");

    sqlx::query(
        "UPDATE organization_product SET status = 'inactive', entitlements = '{\"note\":\"reserved\"}'::jsonb
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, entitlements): (String, serde_json::Value) = sqlx::query_as(
        "SELECT status, entitlements FROM organization_product
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization_id)
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "inactive");
    assert_eq!(entitlements, serde_json::json!({"note": "reserved"}));
}

#[tokio::test]
async fn listados_paginan_por_created_at_e_id_y_filtran() {
    let pool = open(&test_url()).await.unwrap();
    let repository = PgControlRepository::new(pool.clone());
    let prefix = format!("tie-{}", Uuid::new_v4());
    let timestamp = OffsetDateTime::now_utc();
    let mut organization_ids = Vec::new();
    for suffix in ["a", "b", "c"] {
        organization_ids.push(
            insert_organization(&pool, &format!("{prefix}-{suffix}"), "active", timestamp).await,
        );
    }
    organization_ids.sort();

    let first = repository
        .list_organizations(None, Some(&prefix), 2, None)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].id.as_uuid(), organization_ids[0]);
    assert_eq!(first[1].id.as_uuid(), organization_ids[1]);
    let cursor = ControlCursor {
        created_at: first[1].created_at,
        id: first[1].id.as_uuid(),
    };
    let second = repository
        .list_organizations(None, Some(&prefix), 2, Some(cursor))
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id.as_uuid(), organization_ids[2]);

    let account_prefix = format!("account-{}", Uuid::new_v4().simple());
    insert_account(
        &pool,
        &format!("{account_prefix}-alpha@example.test"),
        "Cuenta Alpha",
        "active",
        timestamp,
    )
    .await;
    let disabled_id = insert_account(
        &pool,
        &format!("{account_prefix}-beta@example.test"),
        "Cuenta Beta",
        "disabled",
        timestamp,
    )
    .await;

    let disabled = repository
        .list_accounts(
            Some(AccountStatus::Disabled),
            Some(&account_prefix),
            10,
            None,
        )
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].id.as_uuid(), disabled_id);

    let by_email = repository
        .list_accounts(None, Some(&format!("{account_prefix}-alpha@")), 10, None)
        .await
        .unwrap();
    assert_eq!(by_email.len(), 1);
    assert_eq!(by_email[0].display_name, "Cuenta Alpha");
}

#[tokio::test]
async fn busqueda_es_substring_literal_y_escapa_wildcards() {
    let pool = open(&test_url()).await.unwrap();
    let repository = PgControlRepository::new(pool.clone());
    let prefix = format!("wild-{}", Uuid::new_v4().simple());
    let timestamp = OffsetDateTime::now_utc();
    let literal_percent =
        insert_organization(&pool, &format!("{prefix}-100%-match"), "active", timestamp).await;
    insert_organization(&pool, &format!("{prefix}-100X-match"), "active", timestamp).await;
    let literal_underscore =
        insert_organization(&pool, &format!("{prefix}-a_b"), "active", timestamp).await;
    insert_organization(&pool, &format!("{prefix}-axb"), "active", timestamp).await;

    let percent = repository
        .list_organizations(None, Some(&format!("{prefix}-100%")), 10, None)
        .await
        .unwrap();
    assert_eq!(percent.len(), 1, "% no debe actuar como wildcard");
    assert_eq!(percent[0].id.as_uuid(), literal_percent);

    let underscore = repository
        .list_organizations(None, Some(&format!("{prefix}-a_b")), 10, None)
        .await
        .unwrap();
    assert_eq!(underscore.len(), 1, "_ no debe actuar como wildcard");
    assert_eq!(underscore[0].id.as_uuid(), literal_underscore);
}

#[tokio::test]
async fn detalles_preservan_historial_y_orden_determinista() {
    let pool = open(&test_url()).await.unwrap();
    let repository = PgControlRepository::new(pool.clone());
    let organization_id = insert_organization(
        &pool,
        &format!("detalle-{}", Uuid::new_v4()),
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let first_account = insert_account(
        &pool,
        &format!("first-{}@example.test", Uuid::new_v4().simple()),
        "Primera Cuenta",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let revoked_account = insert_account(
        &pool,
        &format!("revoked-{}@example.test", Uuid::new_v4().simple()),
        "Cuenta Revocada",
        "disabled",
        OffsetDateTime::now_utc(),
    )
    .await;
    let base = OffsetDateTime::now_utc();
    insert_membership(&pool, organization_id, first_account, "owner", base, None).await;
    insert_membership(
        &pool,
        organization_id,
        revoked_account,
        "member",
        base + Duration::seconds(10),
        Some(base + Duration::seconds(20)),
    )
    .await;

    let (first_product, _) = insert_product(&pool, "active").await;
    let (inactive_product, _) = insert_product(&pool, "retired").await;
    insert_organization_product(
        &pool,
        organization_id,
        first_product,
        "active",
        base + Duration::seconds(5),
    )
    .await;
    insert_organization_product(
        &pool,
        organization_id,
        inactive_product,
        "inactive",
        base + Duration::seconds(15),
    )
    .await;

    let detail = repository
        .get_organization(OrganizationId::from_uuid(organization_id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memberships.len(), 2);
    assert_eq!(
        detail.memberships[0].account_id.as_uuid(),
        first_account,
        "orden por joined_at"
    );
    assert_eq!(detail.memberships[1].account_id.as_uuid(), revoked_account);
    assert!(detail.memberships[1].revoked_at.is_some());
    assert_eq!(detail.products.len(), 2);
    assert_eq!(detail.products[0].product_id.as_uuid(), first_product);
    assert_eq!(
        detail.products[1].association_status,
        AssociationStatus::Inactive
    );

    let account_detail = repository
        .get_account(CommercialAccountId::from_uuid(revoked_account))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account_detail.memberships.len(), 1);
    assert_eq!(
        account_detail.memberships[0].organization_id.as_uuid(),
        organization_id
    );
    assert!(account_detail.memberships[0].revoked_at.is_some());

    let missing = repository
        .get_organization(OrganizationId::from_uuid(Uuid::new_v4()))
        .await
        .unwrap();
    assert!(missing.is_none());
    assert_eq!(
        repository
            .list_organizations(
                Some(OrganizationStatus::Archived),
                Some(&format!("no-existe-{}", Uuid::new_v4())),
                10,
                None,
            )
            .await
            .unwrap()
            .len(),
        0
    );
}
