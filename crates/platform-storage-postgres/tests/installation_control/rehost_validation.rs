//! Escenario: validación del request de rehost.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn rehost_valida_request_producto_y_autorizacion() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, _) = insert_product(&pool, "active").await;
    let (other_product, _) = insert_product(&pool, "active").await;
    let (deprecated, _) = insert_product(&pool, "deprecated").await;
    associate_product(&pool, organization, product, "active").await;
    associate_product(&pool, organization, other_product, "active").await;
    associate_product(&pool, organization, deprecated, "active").await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    let service = service(&pool);
    let base = OffsetDateTime::now_utc();

    let installation =
        insert_installation(&pool, organization, Some(product), "active", "active", base).await;
    let identity = insert_identity(&pool, installation).await;
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
    let by_id = InstallationId::from_uuid(installation);

    let pending_request = insert_request(
        &pool,
        installation,
        None,
        None,
        Some(product),
        "pending",
        base,
        base + Duration::minutes(30),
    )
    .await;
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(pending_request),
                "no aprobado".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::RehostConflict)
    );

    let foreign =
        insert_installation(&pool, organization, Some(product), "active", "active", base).await;
    let foreign_request = insert_request(
        &pool,
        foreign,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(foreign_request),
                "otra instalación".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::RehostConflict)
    );

    let wrong_product = insert_request(
        &pool,
        installation,
        Some(organization),
        Some(owner),
        Some(other_product),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(wrong_product),
                "producto ajeno".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::RehostConflict)
    );

    let expired = insert_request(
        &pool,
        installation,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base - Duration::minutes(10),
        base - Duration::minutes(1),
    )
    .await;
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(expired),
                "request expirado".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::RehostConflict)
    );

    // Competidor: una segunda solicitud no puede reemplazar a la pendiente.
    let first = insert_request(
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
    let second = insert_request(
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
    service
        .authorize_rehost(
            &admin,
            by_id,
            EnrollmentRequestId::from_uuid(first),
            "primera".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(second),
                "competidora".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::RehostConflict)
    );

    // Una pendiente vencida se marca expired y deja paso a la nueva.
    let foreign_identity = insert_identity(&pool, foreign).await;
    let foreign_original = insert_request(
        &pool,
        foreign,
        Some(organization),
        Some(owner),
        Some(product),
        "completed",
        base,
        base + Duration::hours(1),
    )
    .await;
    insert_binding(
        &pool,
        organization,
        foreign,
        foreign_original,
        foreign_identity,
        owner,
    )
    .await;
    let stale = insert_request(
        &pool,
        foreign,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base - Duration::minutes(10),
        base + Duration::minutes(30),
    )
    .await;
    let fresh = insert_request(
        &pool,
        foreign,
        Some(organization),
        Some(owner),
        Some(product),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;
    sqlx::query(
        "INSERT INTO installation_rehost_authorization \
         (id, installation_id, enrollment_request_id, organization_id, product_id, \
          authorized_by_staff_account_id, reason, status, created_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'vencida', 'pending', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(foreign)
    .bind(stale)
    .bind(organization)
    .bind(product)
    .bind(admin.account_id.as_uuid())
    .bind(base - Duration::minutes(5))
    .bind(base - Duration::minutes(1))
    .execute(&pool)
    .await
    .unwrap();
    let fresh_view = service
        .authorize_rehost(
            &admin,
            InstallationId::from_uuid(foreign),
            EnrollmentRequestId::from_uuid(fresh),
            "renovada".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(fresh_view.status, RehostStatus::Pending);
    let stale_status: String = sqlx::query_scalar(
        "SELECT status FROM installation_rehost_authorization WHERE enrollment_request_id = $1",
    )
    .bind(stale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_status, "expired");

    // Producto deprecado: la instalación existente no es recuperable.
    sqlx::query("UPDATE product SET status = 'deprecated' WHERE id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    let deprecated_request = insert_request(
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
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                by_id,
                EnrollmentRequestId::from_uuid(deprecated_request),
                "producto deprecado".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::ProductUnavailable)
    );

    // Producto maintenance: permitido para instalación existente.
    sqlx::query("UPDATE product SET status = 'maintenance' WHERE id = $1")
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    // Se libera la autorización competidora antes de reautorizar el mismo rehost.
    sqlx::query(
        "UPDATE installation_rehost_authorization SET status = 'cancelled', cancelled_at = now() \
         WHERE installation_id = $1 AND status = 'pending'",
    )
    .bind(installation)
    .execute(&pool)
    .await
    .unwrap();
    service
        .authorize_rehost(
            &admin,
            by_id,
            EnrollmentRequestId::from_uuid(second),
            "mantenimiento".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    // Asociación inactiva: el producto deja de estar disponible.
    let third = insert_installation(
        &pool,
        organization,
        Some(deprecated),
        "active",
        "active",
        base,
    )
    .await;
    let third_identity = insert_identity(&pool, third).await;
    let third_original = insert_request(
        &pool,
        third,
        Some(organization),
        Some(owner),
        Some(deprecated),
        "completed",
        base,
        base + Duration::hours(1),
    )
    .await;
    insert_binding(
        &pool,
        organization,
        third,
        third_original,
        third_identity,
        owner,
    )
    .await;
    sqlx::query("UPDATE product SET status = 'active' WHERE id = $1")
        .bind(deprecated)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organization_product SET status = 'inactive' \
         WHERE organization_id = $1 AND product_id = $2",
    )
    .bind(organization)
    .bind(deprecated)
    .execute(&pool)
    .await
    .unwrap();
    let inactive_request = insert_request(
        &pool,
        third,
        Some(organization),
        Some(owner),
        Some(deprecated),
        "approved",
        base,
        base + Duration::minutes(30),
    )
    .await;
    assert_eq!(
        service
            .authorize_rehost(
                &admin,
                InstallationId::from_uuid(third),
                EnrollmentRequestId::from_uuid(inactive_request),
                "asociación inactiva".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::ProductUnavailable)
    );
}
