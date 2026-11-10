//! Fixtures compartidos del control de instalaciones.

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

pub(crate) async fn insert_organization(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at) \
         VALUES ($1, $2, 'active', now())",
    )
    .bind(id)
    .bind(format!("C5 org {id}"))
    .execute(pool)
    .await
    .unwrap();
    id
}

pub(crate) async fn insert_owner(pool: &PgPool, organization: Uuid) -> Uuid {
    let account = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
         VALUES ($1, $2, 'Owner C5', now())",
    )
    .bind(account)
    .bind(format!("c5-owner-{account}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership \
         (organization_id, account_id, role, role_version, joined_at) \
         VALUES ($1, $2, 'owner', 1, now())",
    )
    .bind(organization)
    .bind(account)
    .execute(pool)
    .await
    .unwrap();
    account
}

pub(crate) async fn insert_product(pool: &PgPool, status: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let key = format!("c5-product-{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
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

pub(crate) async fn associate_product(
    pool: &PgPool,
    organization: Uuid,
    product: Uuid,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
         VALUES ($1, $2, $3, now())",
    )
    .bind(organization)
    .bind(product)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

pub(crate) async fn insert_staff(pool: &PgPool, role: StaffRole) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_account (id, email, display_name, status, created_at, updated_at) \
         VALUES ($1, $2, $3, 'active', now(), now())",
    )
    .bind(id)
    .bind(format!("c5-staff-{}-{id}@hiqnet.test", role.as_str()))
    .bind(format!("Staff {}", role.as_str()))
    .execute(pool)
    .await
    .unwrap();
    id
}

pub(crate) fn actor(staff: Uuid, role: StaffRole) -> ControlActor {
    ControlActor::new(StaffAccountId::from_uuid(staff), role)
}

pub(crate) async fn insert_installation(
    pool: &PgPool,
    organization: Uuid,
    product: Option<Uuid>,
    status: &str,
    identity_status: &str,
    registered_at: OffsetDateTime,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installation \
         (id, organization_id, product_id, device_label, status, identity_status, registered_at, \
          enrolled_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, \
                 CASE WHEN $6 = 'unenrolled' THEN NULL ELSE $7 END)",
    )
    .bind(id)
    .bind(organization)
    .bind(product)
    .bind(format!("Caja C5 {id}"))
    .bind(status)
    .bind(identity_status)
    .bind(registered_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

pub(crate) async fn insert_identity(pool: &PgPool, installation: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let mut key = [0u8; 32];
    let mut fingerprint = [0u8; 32];
    rand_core::RngCore::fill_bytes(&mut OsRng, &mut key);
    rand_core::RngCore::fill_bytes(&mut OsRng, &mut fingerprint);
    sqlx::query(
        "INSERT INTO installation_identity \
         (id, installation_id, public_key, fingerprint, generation, created_at) \
         VALUES ($1, $2, $3, $4, 1, now())",
    )
    .bind(id)
    .bind(installation)
    .bind(key.to_vec())
    .bind(fingerprint.to_vec())
    .execute(pool)
    .await
    .unwrap();
    id
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_request(
    pool: &PgPool,
    installation: Uuid,
    organization: Option<Uuid>,
    owner: Option<Uuid>,
    product: Option<Uuid>,
    status: &str,
    created_at: OffsetDateTime,
    expires_at: OffsetDateTime,
) -> Uuid {
    let id = Uuid::new_v4();
    let random = |seed: u8| {
        let mut bytes = [seed; 32];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut bytes);
        bytes.to_vec()
    };
    let approved = matches!(status, "approved" | "completed");
    sqlx::query(
        "INSERT INTO enrollment_request \
         (id, installation_id, product_id, public_key, fingerprint, device_label, app_version, \
          platform, device_code_hash, user_code_hash, nonce_hash, status, organization_id, \
          approved_by_account_id, idempotency_scope, idempotency_key, request_hash, \
          created_at, expires_at, approved_at, consumed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, '0.1.0', 'windows', $7, $8, $9, $10, $11, $12, \
                 'c5', NULL, $13, $14, $15, $16, $17)",
    )
    .bind(id)
    .bind(installation)
    .bind(product)
    .bind(random(1))
    .bind(random(2))
    .bind(format!("Caja C5 request {id}"))
    .bind(random(3))
    .bind(random(4))
    .bind(random(5))
    .bind(status)
    .bind(organization)
    .bind(owner)
    .bind(random(6))
    .bind(created_at)
    .bind(expires_at)
    .bind(approved.then_some(created_at))
    .bind((status == "completed").then_some(created_at))
    .execute(pool)
    .await
    .unwrap();
    id
}

pub(crate) async fn insert_binding(
    pool: &PgPool,
    organization: Uuid,
    installation: Uuid,
    request: Uuid,
    identity: Uuid,
    owner: Uuid,
) {
    sqlx::query(
        "INSERT INTO installation_binding \
         (id, organization_id, installation_id, enrollment_request_id, installation_identity_id, \
          approved_by_account_id, bound_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(Uuid::new_v4())
    .bind(organization)
    .bind(installation)
    .bind(request)
    .bind(identity)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
}

pub(crate) fn service(
    pool: &PgPool,
) -> InstallationControlService<PgInstallationControlRepository> {
    InstallationControlService::new(PgInstallationControlRepository::new(pool.clone()))
}

pub(crate) fn enrollment_service(pool: &PgPool) -> EnrollmentService<PgEnrollmentRepository> {
    EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        600,
        "http://127.0.0.1:8080/device".into(),
    )
}

pub(crate) struct DeviceKey {
    pub(crate) signing: ed25519_dalek::SigningKey,
    pub(crate) public_key: String,
}

impl DeviceKey {
    pub(crate) fn new() -> Self {
        let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let public_key = format!(
            "ed25519:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing.verifying_key().as_bytes())
        );
        Self {
            signing,
            public_key,
        }
    }

    pub(crate) fn proof(
        &self,
        challenge: &EnrollmentChallengeView,
        installation: InstallationId,
    ) -> ProofAttempt {
        let message = canonical_proof_message(
            &challenge.device_code,
            installation,
            &self.public_key,
            &challenge.nonce,
        );
        let signature = self.signing.sign(&message);
        ProofAttempt {
            device_code: challenge.device_code.clone(),
            installation_id: installation,
            public_key: self.public_key.clone(),
            nonce: challenge.nonce.clone(),
            signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signature.to_bytes()),
        }
    }
}

pub(crate) async fn enrolar(
    pool: &PgPool,
    organization: Uuid,
    owner: Uuid,
    product_key: &str,
    label: &str,
) -> (InstallationId, DeviceKey, ProofAttempt) {
    let service = enrollment_service(pool);
    let installation = InstallationId::new();
    let key = DeviceKey::new();
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: installation,
                product_key: product_key.into(),
                public_key: key.public_key.clone(),
                device_label: label.into(),
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
    let proof = key.proof(&challenge, installation);
    service
        .prove_enrollment(proof.clone(), OffsetDateTime::now_utc())
        .await
        .unwrap();
    (installation, key, proof)
}

pub(crate) async fn solicitar_rehost(
    pool: &PgPool,
    organization: Uuid,
    owner: Uuid,
    installation: InstallationId,
    product_key: &str,
    label: &str,
) -> (EnrollmentRequestId, DeviceKey, ProofAttempt) {
    let service = enrollment_service(pool);
    let key = DeviceKey::new();
    let challenge = service
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: installation,
                product_key: product_key.into(),
                public_key: key.public_key.clone(),
                device_label: label.into(),
                app_version: "0.2.0".into(),
                platform: "windows".into(),
                idempotency_key: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let approved = service
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
    let proof = key.proof(&challenge, installation);
    (approved.request_id, key, proof)
}

pub(crate) async fn audit_actions(pool: &PgPool, installation: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT action FROM staff_audit_event \
         WHERE resource_type = 'installation' AND resource_id = $1 ORDER BY occurred_at",
    )
    .bind(installation)
    .fetch_all(pool)
    .await
    .unwrap()
}
