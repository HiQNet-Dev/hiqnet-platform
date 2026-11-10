//! Escenario: idempotency.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn idempotencia_rbac_firma_duplicate_key_y_aislamiento() {
    use platform_application::enrollment::EnrollmentError;
    use platform_core::ids::OrganizationId;

    let pool = open(&test_url()).await.unwrap();
    let (organization_id, _, owner) = fixture(&pool).await;
    let service = EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        600,
        "http://127.0.0.1:8080/device".into(),
    );
    let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let public_key = format!(
        "ed25519:v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes())
    );
    let installation = InstallationId::new();
    let key = format!("idem-{installation}");
    let request = BeginEnrollmentRequest {
        installation_id: installation,
        product_key: TEST_PRODUCT_KEY.into(),
        public_key: public_key.clone(),
        device_label: "Caja idem".into(),
        app_version: "0.1.0".into(),
        platform: "linux".into(),
        idempotency_key: Some(key.clone()),
    };
    let challenge = service
        .begin_enrollment(request.clone(), OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert!(matches!(
        service
            .begin_enrollment(request, OffsetDateTime::now_utc())
            .await,
        Err(EnrollmentError::RestartRequired)
    ));
    let different = BeginEnrollmentRequest {
        installation_id: InstallationId::new(),
        product_key: TEST_PRODUCT_KEY.into(),
        public_key: public_key.clone(),
        device_label: "Otra caja".into(),
        app_version: "0.1.0".into(),
        platform: "linux".into(),
        idempotency_key: Some(key),
    };
    assert!(matches!(
        service
            .begin_enrollment(different, OffsetDateTime::now_utc())
            .await,
        Err(EnrollmentError::IdempotencyConflict)
    ));

    let admin_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
         VALUES ($1, $2, 'Admin fixture', now())",
    )
    .bind(admin_id)
    .bind(format!("admin-{admin_id}@example.test"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership \
         (organization_id, account_id, role, role_version, joined_at) \
         VALUES ($1, $2, 'admin', 1, now())",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        service
            .approve_enrollment(
                ApproveEnrollmentRequest {
                    user_code: challenge.user_code.clone(),
                    organization_id: OrganizationId::from_uuid(organization_id),
                    actor_account_id: CommercialAccountId::from_uuid(admin_id),
                },
                OffsetDateTime::now_utc(),
            )
            .await,
        Err(EnrollmentError::Forbidden)
    ));
    let member_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
         VALUES ($1, $2, 'Member fixture', now())",
    )
    .bind(member_id)
    .bind(format!("member-{member_id}@example.test"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership \
         (organization_id, account_id, role, role_version, joined_at) \
         VALUES ($1, $2, 'member', 1, now())",
    )
    .bind(organization_id)
    .bind(member_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        service
            .approve_enrollment(
                ApproveEnrollmentRequest {
                    user_code: challenge.user_code.clone(),
                    organization_id: OrganizationId::from_uuid(organization_id),
                    actor_account_id: CommercialAccountId::from_uuid(member_id),
                },
                OffsetDateTime::now_utc(),
            )
            .await,
        Err(EnrollmentError::Forbidden)
    ));
    service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge.user_code.clone(),
                organization_id: OrganizationId::from_uuid(organization_id),
                actor_account_id: owner,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    let message = canonical_proof_message(
        &challenge.device_code,
        installation,
        &public_key,
        &challenge.nonce,
    );
    let bad_signature = signing.sign(b"mensaje-distinto");
    assert!(matches!(
        service
            .prove_enrollment(
                ProofAttempt {
                    device_code: challenge.device_code.clone(),
                    installation_id: installation,
                    public_key: public_key.clone(),
                    nonce: challenge.nonce.clone(),
                    signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(bad_signature.to_bytes()),
                },
                OffsetDateTime::now_utc(),
            )
            .await,
        Err(EnrollmentError::Invalid) | Err(EnrollmentError::Domain(_))
    ));

    let signature = signing.sign(&message);
    service
        .prove_enrollment(
            ProofAttempt {
                device_code: challenge.device_code,
                installation_id: installation,
                public_key: public_key.clone(),
                nonce: challenge.nonce,
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature.to_bytes()),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    let second = InstallationId::new();
    let challenge2 = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: second,
                product_key: TEST_PRODUCT_KEY.into(),
                public_key: public_key.clone(),
                device_label: "Caja clonada".into(),
                app_version: "0.1.0".into(),
                platform: "linux".into(),
                idempotency_key: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge2.user_code,
                organization_id: OrganizationId::from_uuid(organization_id),
                actor_account_id: owner,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let message2 = canonical_proof_message(
        &challenge2.device_code,
        second,
        &public_key,
        &challenge2.nonce,
    );
    let signature2 = signing.sign(&message2);
    let duplicate_result = service
        .prove_enrollment(
            ProofAttempt {
                device_code: challenge2.device_code,
                installation_id: second,
                public_key,
                nonce: challenge2.nonce,
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature2.to_bytes()),
            },
            OffsetDateTime::now_utc(),
        )
        .await;
    assert!(
        matches!(duplicate_result, Err(EnrollmentError::Conflict)),
        "duplicate key inesperado: {duplicate_result:?}"
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'duplicate_key' AND organization_id = $1",
    )
    .bind(organization_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(events >= 1);

    let other_account = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
         VALUES ($1, $2, 'Other fixture', now())",
    )
    .bind(other_account)
    .bind(format!("other-{other_account}@example.test"))
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        service
            .list_installations(
                CommercialAccountId::from_uuid(other_account),
                OrganizationId::from_uuid(organization_id),
                None,
                50,
            )
            .await,
        Err(EnrollmentError::NotFound)
    ));
}

#[tokio::test]
async fn deshabilitacion_de_cuenta_confirmada_antes_del_proof_impide_binding() {
    use platform_application::enrollment::EnrollmentError;
    use platform_core::ids::OrganizationId;

    let pool = open(&test_url()).await.unwrap();
    let (organization_id, account_id, owner) = fixture(&pool).await;
    let service = EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        600,
        "http://127.0.0.1:8080/device".into(),
    );
    let installation = InstallationId::new();
    let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let public_key = format!(
        "ed25519:v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signing.verifying_key().as_bytes())
    );
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: installation,
                product_key: TEST_PRODUCT_KEY.into(),
                public_key: public_key.clone(),
                device_label: "Caja cuenta deshabilitada".into(),
                app_version: "0.1.0".into(),
                platform: "linux".into(),
                idempotency_key: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge.user_code,
                organization_id: OrganizationId::from_uuid(organization_id),
                actor_account_id: owner,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let message = canonical_proof_message(
        &challenge.device_code,
        installation,
        &public_key,
        &challenge.nonce,
    );
    let signature = signing.sign(&message);
    let proof = ProofAttempt {
        device_code: challenge.device_code,
        installation_id: installation,
        public_key,
        nonce: challenge.nonce,
        signature: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    };

    // La transacción A mantiene el lock de escritura de la cuenta antes de
    // que proof intente el FOR SHARE; no se usa sleep como sincronización.
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account_id)
        .execute(&mut *blocker)
        .await
        .unwrap();

    let proof_pool = pool.clone();
    let proof_task = tokio::spawn(async move {
        let service = EnrollmentService::new(
            PgEnrollmentRepository::new(proof_pool),
            600,
            "http://127.0.0.1:8080/device".into(),
        );
        service
            .prove_enrollment(proof, OffsetDateTime::now_utc())
            .await
    });

    let mut observed = false;
    for _ in 0..200 {
        let blocked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND query LIKE '%commercial_account%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if blocked > 0 {
            observed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(observed, "proof debe esperar el lock de la cuenta");
    blocker.commit().await.unwrap();
    let result = proof_task.await.unwrap();
    assert!(
        matches!(result, Err(EnrollmentError::EnrollmentInvalid)),
        "cuenta deshabilitada debe impedir el binding: {result:?}"
    );
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bindings, 0);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'authorization_revoked_during_proof' AND organization_id = $1",
    )
    .bind(organization_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 1);
}
