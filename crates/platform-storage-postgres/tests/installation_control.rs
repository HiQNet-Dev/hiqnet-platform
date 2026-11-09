//! C5.1D — control administrativo de instalaciones POS.
//!
//! Cubre la migración 0009, el read model keyset, las mutaciones
//! administrativas, la doble autoridad del rehost y el cutover atómico
//! de identidad.

use base64::Engine as _;
use ed25519_dalek::Signer;
use platform_application::enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, EnrollmentChallengeView, EnrollmentError,
    EnrollmentService, ProofAttempt,
};
use platform_application::installation_control::{
    AdministrativeStatus, ControlActor, IdentityStatus, InstallationControlError,
    InstallationControlService, InstallationFilters, RehostStatus,
};
use platform_core::{
    enrollment::canonical_proof_message,
    ids::{
        CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId, ProductId,
        StaffAccountId,
    },
    staff::StaffRole,
};
use platform_storage_postgres::{PgEnrollmentRepository, PgInstallationControlRepository, open};
use rand_core::OsRng;
use sqlx::{PgPool, Row};
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

async fn insert_organization(pool: &PgPool) -> Uuid {
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

async fn insert_owner(pool: &PgPool, organization: Uuid) -> Uuid {
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

async fn insert_product(pool: &PgPool, status: &str) -> (Uuid, String) {
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

async fn associate_product(pool: &PgPool, organization: Uuid, product: Uuid, status: &str) {
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

async fn insert_staff(pool: &PgPool, role: StaffRole) -> Uuid {
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

fn actor(staff: Uuid, role: StaffRole) -> ControlActor {
    ControlActor::new(StaffAccountId::from_uuid(staff), role)
}

async fn insert_installation(
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

async fn insert_identity(pool: &PgPool, installation: Uuid) -> Uuid {
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
async fn insert_request(
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

async fn insert_binding(
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

fn service(pool: &PgPool) -> InstallationControlService<PgInstallationControlRepository> {
    InstallationControlService::new(PgInstallationControlRepository::new(pool.clone()))
}

fn enrollment_service(pool: &PgPool) -> EnrollmentService<PgEnrollmentRepository> {
    EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        600,
        "http://127.0.0.1:8080/device".into(),
    )
}

struct DeviceKey {
    signing: ed25519_dalek::SigningKey,
    public_key: String,
}

impl DeviceKey {
    fn new() -> Self {
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

    fn proof(
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

async fn enrolar(
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

async fn solicitar_rehost(
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

async fn audit_actions(pool: &PgPool, installation: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT action FROM staff_audit_event \
         WHERE resource_type = 'installation' AND resource_id = $1 ORDER BY occurred_at",
    )
    .bind(installation)
    .fetch_all(pool)
    .await
    .unwrap()
}

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

#[tokio::test]
async fn control_lista_filtra_pagina_y_detalla() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let prefix = format!("c5-list-{}", Uuid::new_v4().simple());
    let base = OffsetDateTime::now_utc();
    let mut expected = Vec::new();
    for (index, label) in ["alfa", "beta", "gamma"].iter().enumerate() {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO installation \
             (id, organization_id, product_id, device_label, status, identity_status, registered_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(organization)
        .bind(product)
        .bind(format!("{prefix}-{label}"))
        .bind(if index == 1 { "suspended" } else { "active" })
        .bind(if index == 2 { "unenrolled" } else { "active" })
        .bind(base + Duration::seconds(index as i64))
        .execute(&pool)
        .await
        .unwrap();
        expected.push(id);
    }

    let service = service(&pool);
    let reader = actor(
        insert_staff(&pool, StaffRole::ReadOnly).await,
        StaffRole::ReadOnly,
    );
    let filters = InstallationFilters {
        organization_id: Some(OrganizationId::from_uuid(organization)),
        search: Some(prefix.clone()),
        ..Default::default()
    };
    let first = service
        .list(&reader, filters.clone(), Some(2), None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].id.as_uuid(), expected[0]);
    assert_eq!(first.items[1].id.as_uuid(), expected[1]);
    assert!(first.items[0].product_key.is_some());
    assert_eq!(
        first.items[1].administrative_status,
        AdministrativeStatus::Suspended
    );
    let cursor = first.next_cursor.clone().expect("cursor de continuación");
    let second = service
        .list(&reader, filters.clone(), Some(2), Some(cursor))
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].id.as_uuid(), expected[2]);
    assert_eq!(second.items[0].identity_status, IdentityStatus::Unenrolled);
    assert!(second.next_cursor.is_none());

    let suspended = service
        .list(
            &reader,
            InstallationFilters {
                status: Some(AdministrativeStatus::Suspended),
                organization_id: Some(OrganizationId::from_uuid(organization)),
                search: Some(prefix.clone()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(suspended.items.len(), 1);
    assert_eq!(suspended.items[0].id.as_uuid(), expected[1]);

    let by_product = service
        .list(
            &reader,
            InstallationFilters {
                product_id: Some(ProductId::from_uuid(product)),
                search: Some(prefix.clone()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(by_product.items.len(), 3);

    for invalid in [Some(0), Some(101)] {
        assert_eq!(
            service.list(&reader, filters.clone(), invalid, None).await,
            Err(InstallationControlError::InvalidInput)
        );
    }
    assert_eq!(
        service
            .list(
                &reader,
                filters.clone(),
                None,
                Some("no-es-un-cursor".into())
            )
            .await,
        Err(InstallationControlError::InvalidInput)
    );

    let detail = service
        .get(&reader, InstallationId::from_uuid(expected[0]))
        .await
        .unwrap();
    assert_eq!(detail.summary.id.as_uuid(), expected[0]);
    assert!(detail.current_binding.is_none());
    assert!(detail.rehost.is_none());
    assert!(
        detail.current_identity_created_at.is_none(),
        "sin identidad activa no hay fecha de generación"
    );
    assert_eq!(
        service
            .get(&reader, InstallationId::from_uuid(Uuid::new_v4()))
            .await,
        Err(InstallationControlError::NotFound)
    );
}

#[tokio::test]
async fn suspender_reactivar_son_idempotentes_y_auditan() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let installation = insert_installation(
        &pool,
        organization,
        Some(product),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let legacy = insert_installation(
        &pool,
        organization,
        Some(product),
        "unregistered",
        "unenrolled",
        OffsetDateTime::now_utc(),
    )
    .await;

    let service = service(&pool);
    let reader = actor(
        insert_staff(&pool, StaffRole::Support).await,
        StaffRole::Support,
    );
    let manager = actor(
        insert_staff(&pool, StaffRole::Operator).await,
        StaffRole::Operator,
    );
    let by_id = InstallationId::from_uuid(installation);

    assert_eq!(
        service
            .suspend(&reader, by_id, OffsetDateTime::now_utc())
            .await,
        Err(InstallationControlError::Forbidden),
        "installations.read no concede manage"
    );
    assert_eq!(
        service
            .list(&manager, InstallationFilters::default(), None, None)
            .await
            .map(|page| !page.items.is_empty()),
        Ok(true),
        "installations.manage conserva lectura"
    );

    let suspended = service
        .suspend(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        suspended.summary.administrative_status,
        AdministrativeStatus::Suspended
    );
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended"]
    );

    // Idempotente: no duplica auditoría.
    service
        .suspend(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended"]
    );

    let reactivated = service
        .reactivate(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        reactivated.summary.administrative_status,
        AdministrativeStatus::Active
    );
    service
        .reactivate(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended", "installation_reactivated"]
    );

    assert_eq!(
        service
            .suspend(
                &manager,
                InstallationId::from_uuid(legacy),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::TransitionConflict),
        "unregistered es legacy y no transiciona"
    );
    assert_eq!(
        service
            .suspend(
                &manager,
                InstallationId::from_uuid(Uuid::new_v4()),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::NotFound)
    );
}

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
async fn rehost_completion_conserva_estado_suspended() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, product_key) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;

    let (installation, _, _) =
        enrolar(&pool, organization, owner, &product_key, "Caja suspendida").await;
    sqlx::query("UPDATE installation SET status = 'suspended' WHERE id = $1")
        .bind(installation.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let (rehost_request, _, new_proof) = solicitar_rehost(
        &pool,
        organization,
        owner,
        installation,
        &product_key,
        "Caja suspendida nueva",
    )
    .await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    service(&pool)
        .authorize_rehost(
            &admin,
            installation,
            rehost_request,
            "reemplazo en suspensión".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    enrollment_service(&pool)
        .prove_enrollment(new_proof, OffsetDateTime::now_utc())
        .await
        .unwrap();

    let (status, identity_status): (String, String) =
        sqlx::query_as("SELECT status, identity_status FROM installation WHERE id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        status, "suspended",
        "el rehost no altera el estado administrativo"
    );
    assert_eq!(identity_status, "active");
    let generations: Vec<i32> = sqlx::query_scalar(
        "SELECT generation FROM installation_identity WHERE installation_id = $1 ORDER BY generation",
    )
    .bind(installation.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(generations, vec![1, 2]);
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

#[tokio::test]
async fn rehost_reintentable_tras_quedar_unenrolled() {
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
    let authorization = service
        .authorize_rehost(
            &admin,
            by_id,
            EnrollmentRequestId::from_uuid(first),
            "primer intento".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE installation_rehost_authorization SET created_at = $2 WHERE id = $1")
        .bind(authorization.id)
        .bind(base - Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE installation_rehost_authorization SET expires_at = $2 WHERE id = $1")
        .bind(authorization.id)
        .bind(base - Duration::seconds(1))
        .execute(&pool)
        .await
        .unwrap();

    // La instalación queda unenrolled sin identidad ni binding activos: se puede reautorizar.
    let retry = insert_request(
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
    let renewed = service
        .authorize_rehost(
            &admin,
            by_id,
            EnrollmentRequestId::from_uuid(retry),
            "reintento".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert_eq!(renewed.status, RehostStatus::Pending);
    let first_status: String =
        sqlx::query_scalar("SELECT status FROM installation_rehost_authorization WHERE id = $1")
            .bind(authorization.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(first_status, "expired");
    let state: (String, String) =
        sqlx::query_as("SELECT status, identity_status FROM installation WHERE id = $1")
            .bind(installation)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("active".into(), "unenrolled".into()));

    // Unenrolled sin historial de enrolamiento no es rehosteable.
    let never = insert_installation(
        &pool,
        organization,
        Some(product),
        "active",
        "unenrolled",
        base,
    )
    .await;
    let never_request = insert_request(
        &pool,
        never,
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
                InstallationId::from_uuid(never),
                EnrollmentRequestId::from_uuid(never_request),
                "nunca enrolada".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::TransitionConflict)
    );

    // Estado inconsistente: unenrolled con identidad activa.
    let inconsistent = insert_installation(
        &pool,
        organization,
        Some(product),
        "active",
        "unenrolled",
        base,
    )
    .await;
    sqlx::query("UPDATE installation SET enrolled_at = now() WHERE id = $1")
        .bind(inconsistent)
        .execute(&pool)
        .await
        .unwrap();
    insert_identity(&pool, inconsistent).await;
    let inconsistent_request = insert_request(
        &pool,
        inconsistent,
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
                InstallationId::from_uuid(inconsistent),
                EnrollmentRequestId::from_uuid(inconsistent_request),
                "estado imposible".into(),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::TransitionConflict)
    );
}

#[tokio::test]
async fn rehost_rollback_si_no_hay_identidad_activa() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    let base = OffsetDateTime::now_utc();
    let installation =
        insert_installation(&pool, organization, Some(product), "active", "active", base).await;
    let request = insert_request(
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

    let result = service(&pool)
        .authorize_rehost(
            &admin,
            InstallationId::from_uuid(installation),
            EnrollmentRequestId::from_uuid(request),
            "sin identidad".into(),
            OffsetDateTime::now_utc(),
        )
        .await;
    assert_eq!(result, Err(InstallationControlError::TransitionConflict));

    let authorizations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_rehost_authorization WHERE installation_id = $1",
    )
    .bind(installation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(authorizations, 0, "el cutover fallido no persiste nada");
    let identity_status: String =
        sqlx::query_scalar("SELECT identity_status FROM installation WHERE id = $1")
            .bind(installation)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identity_status, "active", "el rollback conserva el estado");
}

#[tokio::test]
async fn rehost_completion_falla_sin_dejar_estado_parcial() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, product_key) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;

    let (installation, _, _) =
        enrolar(&pool, organization, owner, &product_key, "Caja rollback").await;
    let (rehost_request, _, new_proof) = solicitar_rehost(
        &pool,
        organization,
        owner,
        installation,
        &product_key,
        "Caja rollback nueva",
    )
    .await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    service(&pool)
        .authorize_rehost(
            &admin,
            installation,
            rehost_request,
            "rollback".into(),
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    let suffix = Uuid::new_v4().simple();
    sqlx::query(&format!(
        "CREATE OR REPLACE FUNCTION test_fail_rehost_completion_{suffix}_fn() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
           IF NEW.action = 'installation_rehost_completed' \
              AND NEW.resource_id = '{}'::uuid THEN \
             RAISE EXCEPTION 'fallo sintético de auditoría'; \
           END IF; \
           RETURN NEW; \
         END; $$",
        installation.as_uuid()
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER test_fail_rehost_completion_{suffix} BEFORE INSERT ON staff_audit_event \
         FOR EACH ROW EXECUTE FUNCTION test_fail_rehost_completion_{suffix}_fn()"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let result = enrollment_service(&pool)
        .prove_enrollment(new_proof.clone(), OffsetDateTime::now_utc())
        .await;
    assert!(
        matches!(result, Err(EnrollmentError::Storage)),
        "el fallo de auditoría debe abortar la completion: {result:?}"
    );

    sqlx::query(&format!(
        "DROP TRIGGER test_fail_rehost_completion_{suffix} ON staff_audit_event"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "DROP FUNCTION test_fail_rehost_completion_{suffix}_fn()"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let generations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(generations, 1, "no debe crearse la generación N+1");
    let state: (String, String) =
        sqlx::query_as("SELECT status, identity_status FROM installation WHERE id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, ("active".into(), "unenrolled".into()));
    let authorization_status: String = sqlx::query_scalar(
        "SELECT status FROM installation_rehost_authorization WHERE enrollment_request_id = $1",
    )
    .bind(rehost_request.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(authorization_status, "pending");
    let active_bindings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_bindings, 0);
    let performed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'rehost_performed' AND installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(performed, 0, "no debe quedar evento parcial");

    // Sin el trigger, la misma prueba completa el rehost una sola vez.
    enrollment_service(&pool)
        .prove_enrollment(new_proof, OffsetDateTime::now_utc())
        .await
        .unwrap();
    let generations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(generations, 2);
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_audit_event \
         WHERE action = 'installation_rehost_completed' AND resource_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1);
}

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
