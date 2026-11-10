//! Escenario: autoridad de rehost.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn rehost_conserva_installation_y_exige_doble_autoridad() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, product_key) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;

    let (installation, old_key, old_proof) =
        enrolar(&pool, organization, owner, &product_key, "Caja original").await;
    let request_id: Uuid = sqlx::query_scalar(
        "SELECT enrollment_request_id FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(request_id, Uuid::nil());

    let (rehost_request, _, new_proof) = solicitar_rehost(
        &pool,
        organization,
        owner,
        installation,
        &product_key,
        "Caja reemplazo",
    )
    .await;

    let service = service(&pool);
    let operator = actor(
        insert_staff(&pool, StaffRole::Operator).await,
        StaffRole::Operator,
    );
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    let by_id = InstallationId::from_uuid(installation.as_uuid());

    assert_eq!(
        service
            .authorize_rehost(
                &operator,
                by_id,
                rehost_request,
                "intento sin capability".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::Forbidden),
        "installations.manage no autoriza rehost"
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_rehost_authorization WHERE installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, 0);

    let authorization = service
        .authorize_rehost(
            &admin,
            by_id,
            rehost_request,
            "reemplazo de equipo".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(authorization.status, RehostStatus::Pending);
    assert_eq!(authorization.enrollment_request_id, rehost_request);
    let pending_detail = service.get(&admin, by_id).await.unwrap();
    assert_eq!(
        pending_detail.rehost.as_ref().map(|rehost| rehost.status),
        Some(RehostStatus::Pending),
        "el detalle muestra la autorización pending actual"
    );

    let row = sqlx::query("SELECT status, identity_status FROM installation WHERE id = $1")
        .bind(installation.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("status"), "active");
    assert_eq!(row.get::<String, _>("identity_status"), "unenrolled");
    let retired: Option<OffsetDateTime> = sqlx::query_scalar(
        "SELECT retired_at FROM installation_identity WHERE installation_id = $1 AND generation = 1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(retired.is_some(), "la generación anterior se retira");
    let revoked: Option<OffsetDateTime> = sqlx::query_scalar(
        "SELECT revoked_at FROM installation_binding WHERE installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked.is_some(), "el binding anterior se revoca");

    assert_eq!(
        audit_actions(&pool, installation.as_uuid()).await,
        vec![
            "installation_rehost_authorized",
            "installation_identity_retired",
            "installation_binding_revoked"
        ]
    );
    let (kind, metadata): (String, serde_json::Value) = sqlx::query_as(
        "SELECT kind, metadata FROM commercial_security_event WHERE installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "identity_revoked");
    assert_eq!(
        metadata,
        serde_json::json!({"reason": "reemplazo de equipo"})
    );
    let metadata_text = metadata.to_string();
    for forbidden in ["private", "signature", "nonce", "device_code", "public_key"] {
        assert!(
            !metadata_text.contains(forbidden),
            "metadata de seguridad no debe contener {forbidden}"
        );
    }

    // Retry idempotente por enrollment_request_id: misma autorización y sin auditoría extra.
    let retry = service
        .authorize_rehost(
            &admin,
            by_id,
            rehost_request,
            "reemplazo de equipo".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(retry.id, authorization.id);
    assert_eq!(audit_actions(&pool, installation.as_uuid()).await.len(), 3);

    // Un segundo proof válido crea la generación N+1 y reactiva la identidad.
    enrollment_service(&pool)
        .prove_enrollment(new_proof.clone(), OffsetDateTime::now_utc())
        .await
        .unwrap();
    let state = sqlx::query("SELECT status, identity_status FROM installation WHERE id = $1")
        .bind(installation.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state.get::<String, _>("status"), "active");
    assert_eq!(state.get::<String, _>("identity_status"), "active");
    let generations: Vec<i32> = sqlx::query_scalar(
        "SELECT generation FROM installation_identity WHERE installation_id = $1 ORDER BY generation",
    )
    .bind(installation.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(generations, vec![1, 2]);
    let active_binding: Uuid = sqlx::query_scalar(
        "SELECT installation_identity_id FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let active_identity: Uuid = sqlx::query_scalar(
        "SELECT id FROM installation_identity WHERE installation_id = $1 AND retired_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_binding, active_identity);

    // El cierre del rehost deja exactamente una auditoría staff y un evento de seguridad.
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_audit_event \
         WHERE action = 'installation_rehost_completed' AND resource_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1);
    let performed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'rehost_performed' AND installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(performed, 1);
    assert!(
        audit_actions(&pool, installation.as_uuid())
            .await
            .contains(&"installation_rehost_completed".to_string())
    );

    // La autorización consumida no se reutiliza.
    let consumed = service
        .authorize_rehost(
            &admin,
            by_id,
            rehost_request,
            "reemplazo de equipo".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(consumed.status, RehostStatus::Consumed);
    let completed_detail = service.get(&admin, by_id).await.unwrap();
    assert!(
        completed_detail.rehost.is_none(),
        "una autorización consumed no forma parte del estado actual"
    );
    let stored_status: String = sqlx::query_scalar(
        "SELECT status FROM installation_rehost_authorization \
         WHERE enrollment_request_id = $1",
    )
    .bind(rehost_request.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_status, "consumed");
    let stored_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_rehost_authorization \
         WHERE enrollment_request_id = $1",
    )
    .bind(rehost_request.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored_rows, 1,
        "la fila consumed se conserva como histórico"
    );

    // La clave anterior no revive tras el cutover.
    assert!(
        matches!(
            enrollment_service(&pool)
                .prove_enrollment(old_proof, OffsetDateTime::now_utc())
                .await,
            Err(EnrollmentError::Conflict)
        ),
        "un proof de identidad retirada debe fallar"
    );
    let old_key_replay = enrollment_service(&pool)
        .prove_enrollment(
            ProofAttempt {
                device_code: format!("inventado-{installation}"),
                installation_id: installation,
                public_key: old_key.public_key.clone(),
                nonce: "inventado".into(),
                signature: "inventado".into(),
            },
            OffsetDateTime::now_utc(),
        )
        .await;
    assert!(old_key_replay.is_err());

    // El replay idempotente de la clave vigente sigue funcionando y no duplica el cierre.
    enrollment_service(&pool)
        .prove_enrollment(new_proof, OffsetDateTime::now_utc())
        .await
        .unwrap();
    let completed_after_replay: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_audit_event \
         WHERE action = 'installation_rehost_completed' AND resource_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed_after_replay, 1, "el replay no duplica auditoría");
    let performed_after_replay: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'rehost_performed' AND installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(performed_after_replay, 1, "el replay no duplica el evento");
}

#[tokio::test]
async fn rehost_respeta_estado_administrativo_y_rechaza_estados_invalidos() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    let service = service(&pool);
    let base = OffsetDateTime::now_utc();

    let suspended = insert_installation(
        &pool,
        organization,
        Some(product),
        "suspended",
        "active",
        base,
    )
    .await;
    let identity = insert_identity(&pool, suspended).await;
    let original = insert_request(
        &pool,
        suspended,
        Some(organization),
        Some(owner),
        Some(product),
        "completed",
        base,
        base + Duration::hours(1),
    )
    .await;
    insert_binding(&pool, organization, suspended, original, identity, owner).await;
    let request = insert_request(
        &pool,
        suspended,
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
            InstallationId::from_uuid(suspended),
            EnrollmentRequestId::from_uuid(request),
            "reemplazo".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let state: (String, String) =
        sqlx::query_as("SELECT status, identity_status FROM installation WHERE id = $1")
            .bind(suspended)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("suspended".into(), "unenrolled".into()));

    for (status, identity_status) in [("unregistered", "active"), ("active", "retired")] {
        let id = insert_installation(
            &pool,
            organization,
            Some(product),
            status,
            identity_status,
            base,
        )
        .await;
        if identity_status == "active" {
            let identity = insert_identity(&pool, id).await;
            let original = insert_request(
                &pool,
                id,
                Some(organization),
                Some(owner),
                Some(product),
                "completed",
                base,
                base + Duration::hours(1),
            )
            .await;
            insert_binding(&pool, organization, id, original, identity, owner).await;
        }
        let request = insert_request(
            &pool,
            id,
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
                    InstallationId::from_uuid(id),
                    EnrollmentRequestId::from_uuid(request),
                    "reemplazo".into(),
                    OffsetDateTime::now_utc()
                )
                .await,
            Err(InstallationControlError::TransitionConflict),
            "rehost inválido para {status}/{identity_status}"
        );
    }
}
