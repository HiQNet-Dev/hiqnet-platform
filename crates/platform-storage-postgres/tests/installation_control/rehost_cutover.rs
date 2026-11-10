//! Escenario: rehost y cutover de identidad.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn rehost_idempotente_bajo_concurrencia_real() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let owner = insert_owner(&pool, organization).await;
    let (product, product_key) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;

    let (installation, _, _) =
        enrolar(&pool, organization, owner, &product_key, "Caja concurrente").await;
    let (rehost_request, _, _) = solicitar_rehost(
        &pool,
        organization,
        owner,
        installation,
        &product_key,
        "Caja concurrente nueva",
    )
    .await;
    let admin = actor(
        insert_staff(&pool, StaffRole::PlatformAdmin).await,
        StaffRole::PlatformAdmin,
    );
    let service = service(&pool);
    let by_id = InstallationId::from_uuid(installation.as_uuid());
    let now = OffsetDateTime::now_utc();

    let (first, second) = tokio::join!(
        service.authorize_rehost(
            &admin,
            by_id,
            rehost_request,
            "concurrente".to_string(),
            now
        ),
        service.authorize_rehost(
            &admin,
            by_id,
            rehost_request,
            "concurrente".to_string(),
            now
        ),
    );
    let first = first.expect("primera autorización concurrente");
    let second = second.expect("segunda autorización concurrente");
    assert_eq!(
        first.id, second.id,
        "ambos reintentos devuelven la misma autorización"
    );
    assert_eq!(first.status, RehostStatus::Pending);

    let authorizations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_rehost_authorization \
         WHERE enrollment_request_id = $1",
    )
    .bind(rehost_request.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(authorizations, 1, "una sola fila por enrollment_request_id");
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_rehost_authorization \
         WHERE installation_id = $1 AND status = 'pending'",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, 1);

    let actions = audit_actions(&pool, installation.as_uuid()).await;
    for action in [
        "installation_rehost_authorized",
        "installation_identity_retired",
        "installation_binding_revoked",
    ] {
        assert_eq!(
            actions
                .iter()
                .filter(|value| value.as_str() == action)
                .count(),
            1,
            "{action} debe registrarse exactamente una vez"
        );
    }
    assert_eq!(actions.len(), 3, "sin auditoría duplicada");

    let security_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_security_event \
         WHERE kind = 'identity_revoked' AND installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(security_events, 1);

    let active_identities: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_identity \
         WHERE installation_id = $1 AND retired_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_identities, 0);
    let retired_identities: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_identity \
         WHERE installation_id = $1 AND retired_at IS NOT NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retired_identities, 1);
    let active_bindings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_bindings, 0);
    let revoked_bindings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installation_binding \
         WHERE installation_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revoked_bindings, 1);
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
