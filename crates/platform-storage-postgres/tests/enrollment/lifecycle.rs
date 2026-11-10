//! Escenario: lifecycle.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn enrolamiento_completo_persiste_solo_hashes_y_permite_listar() {
    let pool = open(&test_url()).await.unwrap();
    let (organization_id, _, actor) = fixture(&pool).await;
    let service = EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        600,
        "http://127.0.0.1:8080/device".into(),
    );
    let installation_id = InstallationId::new();
    let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let public_key = format!(
        "ed25519:v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing_key.verifying_key().as_bytes())
    );
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id,
                product_key: TEST_PRODUCT_KEY.into(),
                public_key: public_key.clone(),
                device_label: "Caja fixture".into(),
                app_version: "0.1.0".into(),
                platform: "windows".into(),
                idempotency_key: Some(format!("begin-{installation_id}")),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let approved = service
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code: challenge.user_code.clone(),
                organization_id: OrganizationId::from_uuid(organization_id),
                actor_account_id: actor,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(approved.installation_id, installation_id);

    let message = canonical_proof_message(
        &challenge.device_code,
        installation_id,
        &public_key,
        &challenge.nonce,
    );
    let signature = signing_key.sign(&message);
    let completion = service
        .prove_enrollment(
            ProofAttempt {
                device_code: challenge.device_code.clone(),
                installation_id,
                public_key: public_key.clone(),
                nonce: challenge.nonce.clone(),
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature.to_bytes()),
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(completion.installation_id, installation_id);

    let page = service
        .list_installations(actor, OrganizationId::from_uuid(organization_id), None, 50)
        .await
        .unwrap();
    assert!(
        page.items
            .iter()
            .any(|item| item.installation_id == installation_id)
    );
    assert_eq!(page.items[0].identity_status, "active");

    let status: String =
        sqlx::query_scalar("SELECT status FROM enrollment_request WHERE installation_id = $1")
            .bind(installation_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "completed");
    let raw_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_name = 'enrollment_request' \
           AND column_name IN ('device_code', 'user_code', 'nonce')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw_columns, 0);
    let stored_hashes: (Vec<u8>, Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT device_code_hash, user_code_hash, nonce_hash FROM enrollment_request \
         WHERE installation_id = $1",
    )
    .bind(installation_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(stored_hashes.0),
        challenge.device_code
    );

    let replay = service
        .prove_enrollment(
            ProofAttempt {
                device_code: challenge.device_code,
                installation_id,
                public_key,
                nonce: challenge.nonce,
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature.to_bytes()),
            },
            OffsetDateTime::now_utc(),
        )
        .await;
    assert!(
        replay.is_ok(),
        "el replay idempotente del mismo device devuelve el binding no secreto: {replay:?}"
    );
}

#[tokio::test]
async fn challenge_expirado_no_crea_binding() {
    use platform_application::enrollment::EnrollmentError;
    use platform_core::ids::OrganizationId;

    let pool = open(&test_url()).await.unwrap();
    let (organization_id, _, owner) = fixture(&pool).await;
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
                device_label: "Caja expirada".into(),
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
    sqlx::query(
        "UPDATE enrollment_request SET expires_at = now() - interval '1 second' \
         WHERE installation_id = $1",
    )
    .bind(installation.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    let message = canonical_proof_message(
        &challenge.device_code,
        installation,
        &public_key,
        &challenge.nonce,
    );
    let signature = signing.sign(&message);
    let result = service
        .prove_enrollment(
            ProofAttempt {
                device_code: challenge.device_code,
                installation_id: installation,
                public_key,
                nonce: challenge.nonce,
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature.to_bytes()),
            },
            OffsetDateTime::now_utc(),
        )
        .await;
    assert!(
        matches!(result, Err(EnrollmentError::Expired)),
        "{result:?}"
    );
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bindings, 0);
}
