//! Escenario: enrollment.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn approve_rechaza_producto_atribuido_cambiado_tras_begin() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product_a, key_a) = insert_product(&pool, "active").await;
    let (product_b, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product_a, "active").await;
    associate_product(&pool, organization, product_b, "active").await;
    let service = enrollment_service(&pool);
    let base = OffsetDateTime::now_utc();

    let legacy = insert_installation(&pool, organization, None, "active", "unenrolled", base).await;
    let key = DeviceKey::new();
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: InstallationId::from_uuid(legacy),
                product_key: key_a,
                public_key: key.public_key.clone(),
                device_label: "Caja legacy race".into(),
                app_version: "0.1.0".into(),
                platform: "windows".into(),
                idempotency_key: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    // Carrera: la atribución write-once cambia entre begin y approve.
    sqlx::query("UPDATE installation SET product_id = $2 WHERE id = $1")
        .bind(legacy)
        .bind(product_b)
        .execute(&pool)
        .await
        .unwrap();

    let result = service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge.user_code,
                organization_id: OrganizationId::from_uuid(organization),
                actor_account_id: CommercialAccountId::from_uuid(owner),
            },
            OffsetDateTime::now_utc(),
        )
        .await;
    assert!(
        matches!(result, Err(EnrollmentError::EnrollmentInvalid)),
        "approve debe rechazar el Product incoherente: {result:?}"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM enrollment_request WHERE installation_id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending", "el approve rechazado no muta el request");
    let product: Uuid = sqlx::query_scalar("SELECT product_id FROM installation WHERE id = $1")
        .bind(legacy)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(product, product_b);
}

#[tokio::test]
async fn proof_rechaza_request_con_producto_distinto_y_no_muta_nada() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product_a, _) = insert_product(&pool, "active").await;
    let (product_b, key_b) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product_a, "active").await;
    associate_product(&pool, organization, product_b, "active").await;
    let base = OffsetDateTime::now_utc();

    // Instalación legacy sin Product; el request se crea y aprueba con B.
    let legacy = insert_installation(&pool, organization, None, "active", "unenrolled", base).await;
    let (request, _, proof) = solicitar_rehost(
        &pool,
        organization,
        owner,
        InstallationId::from_uuid(legacy),
        &key_b,
        "Caja legacy proof",
    )
    .await;

    // Atribución posterior a A (NULL → A, write-once).
    sqlx::query("UPDATE installation SET product_id = $2 WHERE id = $1")
        .bind(legacy)
        .bind(product_a)
        .execute(&pool)
        .await
        .unwrap();

    // Autorización histórica directa, previa a la atribución a A.
    let staff = insert_staff(&pool, StaffRole::PlatformAdmin).await;
    let request_expires: OffsetDateTime =
        sqlx::query_scalar("SELECT expires_at FROM enrollment_request WHERE id = $1")
            .bind(request.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO installation_rehost_authorization \
         (id, installation_id, enrollment_request_id, organization_id, product_id, \
          authorized_by_staff_account_id, reason, status, created_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'histórica', 'pending', $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(legacy)
    .bind(request.as_uuid())
    .bind(organization)
    .bind(product_b)
    .bind(staff)
    .bind(base)
    .bind(request_expires)
    .execute(&pool)
    .await
    .unwrap();

    let result = enrollment_service(&pool)
        .prove_enrollment(proof, OffsetDateTime::now_utc())
        .await;
    assert!(
        matches!(result, Err(EnrollmentError::Conflict)),
        "el proof debe rechazar el Product incoherente: {result:?}"
    );

    let identities: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identities, 0, "no debe crearse identidad");
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bindings, 0, "no debe crearse binding");
    let (authorization_status, consumed_at): (String, Option<OffsetDateTime>) = sqlx::query_as(
        "SELECT status, consumed_at FROM installation_rehost_authorization \
         WHERE enrollment_request_id = $1",
    )
    .bind(request.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(authorization_status, "pending");
    assert!(consumed_at.is_none(), "la autorización no se consume");
    let staff_audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM staff_audit_event WHERE resource_id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(staff_audits, 0, "no debe generarse auditoría");
    let security_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event WHERE installation_id = $1",
    )
    .bind(legacy)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(security_events, 0, "no debe generarse evento de seguridad");
    let request_status: String =
        sqlx::query_scalar("SELECT status FROM enrollment_request WHERE id = $1")
            .bind(request.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(request_status, "approved", "el request no se consume");
    let (installed_product, identity_status): (Option<Uuid>, String) =
        sqlx::query_as("SELECT product_id, identity_status FROM installation WHERE id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(installed_product, Some(product_a));
    assert_eq!(identity_status, "unenrolled");
}

#[tokio::test]
async fn proof_atribuye_write_once_el_producto_de_una_instalacion_legacy() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, product_key) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let service = enrollment_service(&pool);
    let base = OffsetDateTime::now_utc();

    let legacy = insert_installation(
        &pool,
        organization,
        None,
        "unregistered",
        "unenrolled",
        base,
    )
    .await;
    let key = DeviceKey::new();
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: InstallationId::from_uuid(legacy),
                product_key,
                public_key: key.public_key.clone(),
                device_label: "Caja legacy atribuible".into(),
                app_version: "0.1.0".into(),
                platform: "windows".into(),
                idempotency_key: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge.user_code.clone(),
                organization_id: OrganizationId::from_uuid(organization),
                actor_account_id: CommercialAccountId::from_uuid(owner),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    service
        .prove_enrollment(
            key.proof(&challenge, InstallationId::from_uuid(legacy)),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    let (installed, identity_status, administrative_status): (Option<Uuid>, String, String) =
        sqlx::query_as(
            "SELECT product_id, identity_status, status FROM installation WHERE id = $1",
        )
        .bind(legacy)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        installed,
        Some(product),
        "un Enrollment válido hace el write-once NULL → Product"
    );
    assert_eq!(identity_status, "active");
    assert_eq!(
        administrative_status, "active",
        "un legacy unregistered se normaliza a active al completar enrollment"
    );
    let generations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(legacy)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(generations, 1);
    let active_bindings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(legacy)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_bindings, 1);
}

#[tokio::test]
async fn enrolment_exige_allowlist_de_producto_desde_begin() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (active, active_key) = insert_product(&pool, "active").await;
    let (maintenance, maintenance_key) = insert_product(&pool, "maintenance").await;
    let (deprecated, deprecated_key) = insert_product(&pool, "deprecated").await;
    let (retired, retired_key) = insert_product(&pool, "retired").await;
    let (other_active, other_active_key) = insert_product(&pool, "active").await;
    for product in [active, maintenance, deprecated, retired, other_active] {
        associate_product(&pool, organization, product, "active").await;
    }
    let service = enrollment_service(&pool);
    let begin =
        |installation: InstallationId, product_key: &str, label: &str| BeginEnrollmentRequest {
            installation_id: installation,
            product_key: product_key.into(),
            public_key: DeviceKey::new().public_key,
            device_label: label.into(),
            app_version: "0.1.0".into(),
            platform: "windows".into(),
            idempotency_key: None,
        };

    // Instalación nueva: sólo Product active.
    assert!(
        service
            .begin_enrollment(
                begin(InstallationId::new(), &active_key, "nueva activa"),
                OffsetDateTime::now_utc()
            )
            .await
            .is_ok()
    );
    for (key, case) in [
        (&maintenance_key, "nueva maintenance"),
        (&deprecated_key, "nueva deprecated"),
        (&retired_key, "nueva retired"),
    ] {
        assert!(
            matches!(
                service
                    .begin_enrollment(
                        begin(InstallationId::new(), key, case),
                        OffsetDateTime::now_utc()
                    )
                    .await,
                Err(EnrollmentError::Invalid)
            ),
            "{case} debe rechazarse en begin"
        );
    }
    let unknown = format!("inexistente-{}", Uuid::new_v4());
    assert!(matches!(
        service
            .begin_enrollment(
                begin(InstallationId::new(), &unknown, "nueva inexistente"),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(EnrollmentError::Invalid)
    ));

    // Instalación existente (rehost): cada una sólo admite su propio Product.
    let existing = insert_installation(
        &pool,
        organization,
        Some(active),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let existing_maintenance = insert_installation(
        &pool,
        organization,
        Some(maintenance),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    for (installation, key, case) in [
        (existing, &active_key, "existente activa"),
        (
            existing_maintenance,
            &maintenance_key,
            "existente maintenance",
        ),
    ] {
        assert!(
            service
                .begin_enrollment(
                    begin(InstallationId::from_uuid(installation), key, case),
                    OffsetDateTime::now_utc()
                )
                .await
                .is_ok(),
            "{case} debe permitirse en begin"
        );
    }
    let existing_deprecated = insert_installation(
        &pool,
        organization,
        Some(deprecated),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let existing_retired = insert_installation(
        &pool,
        organization,
        Some(retired),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    for (installation, key, case) in [
        (existing, &deprecated_key, "existente deprecated"),
        (existing, &other_active_key, "existente otro producto"),
        (
            existing_deprecated,
            &deprecated_key,
            "existente con producto deprecated",
        ),
        (
            existing_retired,
            &retired_key,
            "existente con producto retired",
        ),
    ] {
        assert!(
            matches!(
                service
                    .begin_enrollment(
                        begin(InstallationId::from_uuid(installation), key, case),
                        OffsetDateTime::now_utc()
                    )
                    .await,
                Err(EnrollmentError::Invalid)
            ),
            "{case} debe rechazarse en begin"
        );
    }
}
