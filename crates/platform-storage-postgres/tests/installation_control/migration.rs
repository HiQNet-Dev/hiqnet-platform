//! Escenario: migration.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn migracion_0009_protege_atribucion_historial_y_delete() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (product, _) = insert_product(&pool, "active").await;
    let (other_product, _) = insert_product(&pool, "active").await;
    let (unassociated, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    associate_product(&pool, organization, other_product, "active").await;

    let installation = insert_installation(
        &pool,
        organization,
        None,
        "active",
        "unenrolled",
        OffsetDateTime::now_utc(),
    )
    .await;

    sqlx::query("UPDATE installation SET product_id = $2 WHERE id = $1")
        .bind(installation)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();

    // mismo UUID: no-op permitido.
    sqlx::query("UPDATE installation SET product_id = $2 WHERE id = $1")
        .bind(installation)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();

    let change = sqlx::query("UPDATE installation SET product_id = $2 WHERE id = $1")
        .bind(installation)
        .bind(other_product)
        .execute(&pool)
        .await;
    assert!(change.is_err(), "product_id es write-once");

    let tenant = sqlx::query("UPDATE installation SET organization_id = $2 WHERE id = $1")
        .bind(installation)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await;
    assert!(tenant.is_err(), "organization_id es inmutable");

    let delete = sqlx::query("DELETE FROM installation WHERE id = $1")
        .bind(installation)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "installation no admite DELETE");

    // FK compuesta: el producto debe pertenecer a la organización.
    let invalid = sqlx::query(
        "INSERT INTO installation (id, organization_id, product_id, device_label, status, registered_at) \
         VALUES ($1, $2, $3, $4, 'active', now())",
    )
    .bind(Uuid::new_v4())
    .bind(organization)
    .bind(unassociated)
    .bind(format!("huérfana {}", Uuid::new_v4()))
    .execute(&pool)
    .await;
    assert!(invalid.is_err(), "producto no asociado a la organización");

    // Autorización de rehost: una sola pendiente y expiración acotada por el request.
    let owner = insert_owner(&pool, organization).await;
    let identity = insert_identity(&pool, installation).await;
    let base = OffsetDateTime::now_utc();
    let original = insert_request(
        &pool,
        installation,
        Some(organization),
        Some(owner),
        Some(product),
        "completed",
        base,
        base + Duration::hours(1),
    )
    .await;
    insert_binding(&pool, organization, installation, original, identity, owner).await;
    let rehost_request = insert_request(
        &pool,
        installation,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;

    let insert_authorization = || {
        sqlx::query(
            "INSERT INTO installation_rehost_authorization \
             (id, installation_id, enrollment_request_id, organization_id, product_id, \
              authorized_by_staff_account_id, reason, status, created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, 'fixture', 'pending', $7, $8)",
        )
    };

    let staff = insert_staff(&pool, StaffRole::PlatformAdmin).await;
    let invalid_expiry = insert_authorization()
        .bind(Uuid::new_v4())
        .bind(installation)
        .bind(rehost_request)
        .bind(organization)
        .bind(product)
        .bind(staff)
        .bind(base)
        .bind(base - Duration::seconds(1))
        .execute(&pool)
        .await;
    assert!(
        invalid_expiry.is_err(),
        "expires_at debe ser posterior a created_at"
    );

    let beyond_request = insert_authorization()
        .bind(Uuid::new_v4())
        .bind(installation)
        .bind(rehost_request)
        .bind(organization)
        .bind(product)
        .bind(staff)
        .bind(base)
        .bind(base + Duration::hours(2))
        .execute(&pool)
        .await;
    assert!(
        beyond_request.is_err(),
        "la autorización no puede exceder el challenge"
    );

    insert_authorization()
        .bind(Uuid::new_v4())
        .bind(installation)
        .bind(rehost_request)
        .bind(organization)
        .bind(product)
        .bind(staff)
        .bind(base)
        .bind(base + Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();

    let second_request = insert_request(
        &pool,
        installation,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;
    let duplicate_pending = insert_authorization()
        .bind(Uuid::new_v4())
        .bind(installation)
        .bind(second_request)
        .bind(organization)
        .bind(product)
        .bind(staff)
        .bind(base)
        .bind(base + Duration::minutes(10))
        .execute(&pool)
        .await;
    assert!(
        duplicate_pending.is_err(),
        "sólo una autorización pendiente por instalación"
    );
}
