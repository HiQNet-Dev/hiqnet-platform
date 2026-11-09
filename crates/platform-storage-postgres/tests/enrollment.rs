use base64::Engine as _;
use ed25519_dalek::Signer;
use platform_application::enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, EnrollmentService, ProofAttempt,
};
use platform_core::{
    enrollment::canonical_proof_message,
    ids::{CommercialAccountId, InstallationId, OrganizationId},
};
use platform_storage_postgres::{PgEnrollmentRepository, open};
use rand_core::OsRng;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

const TEST_PRODUCT_KEY: &str = "enrollment-fixture-product";

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

async fn fixture(pool: &PgPool) -> (Uuid, Uuid, CommercialAccountId) {
    let account_id = Uuid::new_v4();
    let organization_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
         VALUES ($1, $2, 'Fixture enrollment', now())",
    )
    .bind(account_id)
    .bind(format!("enrollment-{account_id}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at) \
         VALUES ($1, $2, 'active', now())",
    )
    .bind(organization_id)
    .bind(format!("Org enrollment {organization_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership \
         (organization_id, account_id, role, role_version, joined_at) \
         VALUES ($1, $2, 'owner', 1, now())",
    )
    .bind(organization_id)
    .bind(account_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
         VALUES ($1, $2, 'Fixture product', '', 'active', 'saas', now(), now()) \
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(TEST_PRODUCT_KEY)
    .execute(pool)
    .await
    .unwrap();
    let product_id: Uuid = sqlx::query_scalar("SELECT id FROM product WHERE key = $1")
        .bind(TEST_PRODUCT_KEY)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
         VALUES ($1, $2, 'active', now()) ON CONFLICT DO NOTHING",
    )
    .bind(organization_id)
    .bind(product_id)
    .execute(pool)
    .await
    .unwrap();
    (
        organization_id,
        account_id,
        CommercialAccountId::from_uuid(account_id),
    )
}

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

async fn approved_proof_fixture(
    pool: &PgPool,
    label: &str,
) -> (Uuid, CommercialAccountId, ProofAttempt, InstallationId) {
    use platform_core::ids::OrganizationId;

    let (organization_id, _, owner) = fixture(pool).await;
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
                device_label: label.into(),
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
    (
        organization_id,
        owner,
        ProofAttempt {
            device_code: challenge.device_code,
            installation_id: installation,
            public_key,
            nonce: challenge.nonce,
            signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signature.to_bytes()),
        },
        installation,
    )
}

async fn wait_for_organization_lock(pool: &PgPool) {
    for _ in 0..200 {
        let blocked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND query LIKE '%organization%'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if blocked > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("proof debe esperar el lock de organización");
}

async fn prove_in_task(
    pool: PgPool,
    proof: ProofAttempt,
) -> Result<
    platform_application::enrollment::EnrollmentCompletion,
    platform_application::enrollment::EnrollmentError,
> {
    let service = EnrollmentService::new(
        PgEnrollmentRepository::new(pool),
        600,
        "http://127.0.0.1:8080/device".into(),
    );
    service
        .prove_enrollment(proof, OffsetDateTime::now_utc())
        .await
}

#[tokio::test]
async fn revocacion_cambio_de_rol_y_archivado_concurrentes_impiden_binding() {
    use platform_application::enrollment::EnrollmentError;

    let pool = open(&test_url()).await.unwrap();

    let (organization_id, owner, proof, installation) =
        approved_proof_fixture(&pool, "Caja revocada").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET revoked_at = now() \
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization_id)
    .bind(owner.as_uuid())
    .execute(&mut *blocker)
    .await
    .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    let (organization_id, owner, proof, installation) =
        approved_proof_fixture(&pool, "Caja rol cambiado").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' \
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization_id)
    .bind(owner.as_uuid())
    .execute(&mut *blocker)
    .await
    .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    let (organization_id, _, proof, installation) =
        approved_proof_fixture(&pool, "Caja organización archivada").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("UPDATE organization SET status = 'archived' WHERE id = $1")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn dos_proof_simultaneos_crean_un_unico_binding() {
    let pool = open(&test_url()).await.unwrap();
    let (_, _, proof, installation) = approved_proof_fixture(&pool, "Caja dos proof").await;
    let first = tokio::spawn(prove_in_task(pool.clone(), proof.clone()));
    let second = tokio::spawn(prove_in_task(pool.clone(), proof));
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert!(
        first.is_ok() || second.is_ok(),
        "al menos uno debe completar o reproducir el binding"
    );
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bindings, 1);
    let identities: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identities, 1);
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM enrollment_request \
         WHERE installation_id = $1 AND status = 'completed'",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1);
}
