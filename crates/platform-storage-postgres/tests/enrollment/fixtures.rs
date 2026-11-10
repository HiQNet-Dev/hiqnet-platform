//! Fixtures del enrolamiento.

use super::*;

pub(crate) fn test_url() -> String {
    let url = std::env::var("TEST_DATABASE_URL").expect("base aislada requerida");
    assert!(
        url.rsplit('/')
            .next()
            .is_some_and(|database| database.contains("test") && database != "hiqnet"),
        "TEST_DATABASE_URL debe apuntar a una base aislada"
    );
    url
}

pub(crate) async fn fixture(pool: &PgPool) -> (Uuid, Uuid, CommercialAccountId) {
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

pub(crate) async fn approved_proof_fixture(
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

pub(crate) async fn wait_for_organization_lock(pool: &PgPool) {
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

pub(crate) async fn prove_in_task(
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
