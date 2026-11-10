use platform_application::audit_security::{
    AuditFilters, AuditResult, AuditSecurityActor, AuditSecurityService, SecurityFilters,
    SecurityKind, StaffAuthAction, StaffAuthFilters,
};
use platform_core::{
    ids::{OrganizationId, StaffAccountId},
    staff::StaffRole,
};
use platform_storage_postgres::{PgAuditSecurityRepository, open};
use sqlx::PgPool;
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

struct Fixture {
    staff_account: Uuid,
    organization: Uuid,
    product: Uuid,
    project: Uuid,
    installation: Uuid,
    commercial_account: Uuid,
}

async fn seed(pool: &PgPool, now: OffsetDateTime) -> Fixture {
    let staff_account = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_account (id, email, display_name, status, created_at, updated_at)
         VALUES ($1, $2, 'Security Auditor', 'active', $3, $3)",
    )
    .bind(staff_account)
    .bind(format!("audit-{}@hiqnet.test", staff_account))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let organization = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
         VALUES ($1, $2, 'active', $3)",
    )
    .bind(organization)
    .bind(format!("Audit org {organization}"))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let product = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
         VALUES ($1, $2, $3, '', 'active', 'desktop', $4, $4)",
    )
    .bind(product)
    .bind(format!("audit-{}", &product.to_string()[..8]))
    .bind(format!("Audit product {product}"))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO project (id, key, name, description, project_type, status, product_id, created_at, updated_at)
         VALUES ($1, $2, $3, '', 'application', 'active', $4, $5, $5)",
    )
    .bind(project)
    .bind(format!("audit-project-{}", &project.to_string()[..8]))
    .bind(format!("Audit project {project}"))
    .bind(product)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let installation = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
         VALUES ($1, $2, 'active', $3)",
    )
    .bind(organization)
    .bind(product)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO installation (id, organization_id, product_id, device_label, status, registered_at)
         VALUES ($1, $2, $3, $4, 'active', $5)",
    )
    .bind(installation)
    .bind(organization)
    .bind(product)
    .bind(format!("Audit device {}", &installation.to_string()[..8]))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let commercial_account = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at)
         VALUES ($1, $2, 'Commercial E2E', $3)",
    )
    .bind(commercial_account)
    .bind(format!(
        "audit-commercial-{commercial_account}@example.test"
    ))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    Fixture {
        staff_account,
        organization,
        product,
        project,
        installation,
        commercial_account,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit(
    pool: &PgPool,
    fixture: &Fixture,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    result: &str,
    occurred_at: OffsetDateTime,
    metadata: serde_json::Value,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_audit_event
         (id, staff_account_id, action, resource_type, resource_id, organization_id, product_id,
          result, occurred_at, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(fixture.staff_account)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(fixture.organization)
    .bind(fixture.product)
    .bind(result)
    .bind(occurred_at)
    .bind(metadata)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn audit_listing_uses_keyset_filters_joins_and_sanitization() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let first = insert_audit(
        &pool,
        &fixture,
        "installation_suspended",
        "installation",
        Some(fixture.installation),
        "success",
        now - Duration::seconds(2),
        serde_json::json!({
            "before": "active",
            "after": "suspended",
            "token_hash": "never"
        }),
    )
    .await;
    insert_audit(
        &pool,
        &fixture,
        "product_updated",
        "product",
        Some(fixture.product),
        "denied",
        now - Duration::seconds(1),
        serde_json::json!({ "secret": "never" }),
    )
    .await;
    insert_audit(
        &pool,
        &fixture,
        "project_created",
        "project",
        Some(fixture.project),
        "success",
        now,
        serde_json::json!({}),
    )
    .await;

    let service = AuditSecurityService::new(PgAuditSecurityRepository::new(pool.clone()));
    let actor = AuditSecurityActor::new(
        StaffAccountId::from_uuid(fixture.staff_account),
        StaffRole::SecurityAuditor,
    );
    let filters = AuditFilters {
        organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
        ..AuditFilters::default()
    };
    let page = service
        .list_audit(&actor, filters.clone(), Some(2), None)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].action, "project_created");
    assert_eq!(
        page.items[0].resource_label.as_deref(),
        Some(format!("Audit project {}", fixture.project).as_str())
    );
    assert!(page.next_cursor.is_some());

    let second_page = service
        .list_audit(&actor, filters.clone(), Some(2), page.next_cursor)
        .await
        .unwrap();
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].id, first);
    assert!(second_page.next_cursor.is_none());

    let only_denied = service
        .list_audit(
            &actor,
            AuditFilters {
                organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
                result: Some(AuditResult::Denied),
                ..AuditFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(only_denied.items.len(), 1);
    assert_eq!(only_denied.items[0].result, AuditResult::Denied);

    let detail = service.get_audit(&actor, first).await.unwrap();
    assert_eq!(
        detail.metadata,
        serde_json::json!({ "before": "active", "after": "suspended" })
    );
    assert_eq!(
        detail.actor.unwrap().email,
        format!("audit-{}@hiqnet.test", fixture.staff_account)
    );

    let by_action = service
        .list_audit(
            &actor,
            AuditFilters {
                organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
                action: Some("product_updated".into()),
                ..AuditFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(by_action.items.len(), 1);
    assert_eq!(
        by_action.items[0].resource_label.as_deref(),
        Some(format!("Audit product {}", fixture.product).as_str())
    );
}

#[tokio::test]
async fn security_and_staff_auth_are_separate_datasets() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;

    let rehost = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_security_event
         (id, kind, organization_id, account_id, installation_id, metadata, created_at)
         VALUES ($1, 'rehost_performed', $2, $3, $4, $5, $6)",
    )
    .bind(rehost)
    .bind(fixture.organization)
    .bind(fixture.commercial_account)
    .bind(fixture.installation)
    .bind(serde_json::json!({
        "enrollment_request_id": "request-id",
        "nonce": "never"
    }))
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO commercial_security_event
         (id, kind, organization_id, account_id, installation_id, metadata, created_at)
         VALUES ($1, 'duplicate_key', $2, $3, $4, '{}', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(fixture.organization)
    .bind(fixture.commercial_account)
    .bind(fixture.installation)
    .bind(now - Duration::seconds(1))
    .execute(&pool)
    .await
    .unwrap();

    let staff_auth = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_auth_event (id, account_id, action, occurred_at, metadata)
         VALUES ($1, $2, 'login_failure', $3, $4)",
    )
    .bind(staff_auth)
    .bind(fixture.staff_account)
    .bind(now)
    .bind(serde_json::json!({ "reason": "bad_password", "ip_hash": "never" }))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO staff_auth_event (id, account_id, action, occurred_at, metadata)
         VALUES ($1, $2, 'login_success', $3, '{\"token_hash\":\"never\"}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(fixture.staff_account)
    .bind(now - Duration::seconds(1))
    .execute(&pool)
    .await
    .unwrap();

    let service = AuditSecurityService::new(PgAuditSecurityRepository::new(pool.clone()));
    let actor = AuditSecurityActor::new(
        StaffAccountId::from_uuid(fixture.staff_account),
        StaffRole::SecurityAuditor,
    );

    let security = service
        .list_security(
            &actor,
            SecurityFilters {
                organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
                ..SecurityFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(security.items.len(), 2);
    assert_eq!(security.items[0].kind, SecurityKind::RehostPerformed);
    assert_eq!(
        security.items[0]
            .installation
            .as_ref()
            .unwrap()
            .device_label,
        format!("Audit device {}", &fixture.installation.to_string()[..8])
    );

    let detail = service.get_security(&actor, rehost).await.unwrap();
    assert_eq!(
        detail.metadata,
        serde_json::json!({ "enrollment_request_id": "request-id" })
    );
    assert_eq!(
        detail.account.unwrap().email,
        format!(
            "audit-commercial-{}@example.test",
            fixture.commercial_account
        )
    );

    let by_kind = service
        .list_security(
            &actor,
            SecurityFilters {
                organization_id: Some(OrganizationId::from_uuid(fixture.organization)),
                kind: Some(SecurityKind::DuplicateKey),
                ..SecurityFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(by_kind.items.len(), 1);

    let auth = service
        .list_staff_auth(
            &actor,
            StaffAuthFilters {
                account_id: Some(StaffAccountId::from_uuid(fixture.staff_account)),
                action: Some(StaffAuthAction::LoginFailure),
                ..StaffAuthFilters::default()
            },
            Some(10),
            None,
        )
        .await
        .unwrap();
    assert_eq!(auth.items.len(), 1);
    assert_eq!(auth.items[0].action, StaffAuthAction::LoginFailure);

    let auth_detail = service.get_staff_auth(&actor, staff_auth).await.unwrap();
    assert_eq!(
        auth_detail.metadata,
        serde_json::json!({ "reason": "bad_password" })
    );
}

#[tokio::test]
async fn append_only_triggers_and_read_indexes_exist() {
    let pool = open(&test_url()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    let fixture = seed(&pool, now).await;
    let audit = insert_audit(
        &pool,
        &fixture,
        "installation_identity_retired",
        "installation",
        Some(fixture.installation),
        "success",
        now,
        serde_json::json!({}),
    )
    .await;

    assert!(
        sqlx::query("UPDATE staff_audit_event SET result = 'failure' WHERE id = $1")
            .bind(audit)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM staff_audit_event WHERE id = $1")
            .bind(audit)
            .execute(&pool)
            .await
            .is_err()
    );

    let security = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_security_event (id, kind, metadata, created_at)
         VALUES ($1, 'identity_revoked', '{}', $2)",
    )
    .bind(security)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE commercial_security_event SET kind = 'duplicate_key' WHERE id = $1")
            .bind(security)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM commercial_security_event WHERE id = $1")
            .bind(security)
            .execute(&pool)
            .await
            .is_err()
    );

    let auth = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_auth_event (id, account_id, action, occurred_at, metadata)
         VALUES ($1, $2, 'logout', $3, '{}')",
    )
    .bind(auth)
    .bind(fixture.staff_account)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE staff_auth_event SET action = 'logout' WHERE id = $1")
            .bind(auth)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM staff_auth_event WHERE id = $1")
            .bind(auth)
            .execute(&pool)
            .await
            .is_err()
    );

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' AND indexname = ANY($1)",
    )
    .bind(vec![
        "staff_audit_event_occurred_idx",
        "staff_audit_event_action_occurred_idx",
        "staff_audit_event_org_occurred_idx",
        "staff_audit_event_product_occurred_idx",
        "commercial_security_event_created_idx",
        "commercial_security_event_kind_created_idx",
        "commercial_security_event_installation_created_idx",
        "commercial_security_event_account_created_idx",
        "staff_auth_event_occurred_idx",
    ])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(indexes.len(), 9);
}
