//! Pruebas de control_directory en el Control Center.

use super::common::*;
use super::*;

async fn seed_control_fixture(pool: &PgPool) -> (String, Uuid) {
    let prefix = format!("c4-{}", Uuid::new_v4().simple());
    let timestamp = time::OffsetDateTime::now_utc();
    for (index, status) in ["active", "legacy_unclaimed", "archived"]
        .iter()
        .enumerate()
    {
        sqlx::query(
                "INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, $2, $3, $4)",
            )
            .bind(Uuid::new_v4())
            .bind(format!("{prefix}-org-{index}"))
            .bind(*status)
            .bind(timestamp)
            .execute(pool)
            .await
            .unwrap();
    }
    let organization_id = Uuid::new_v4();
    sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, $2, 'active', $3)",
        )
        .bind(organization_id)
        .bind(format!("{prefix}-detalle"))
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
    let first_account = Uuid::new_v4();
    let revoked_account = Uuid::new_v4();
    for (id, email, name, status) in [
        (
            first_account,
            format!("{prefix}-first@example.test"),
            "Primera Cuenta".to_owned(),
            "active",
        ),
        (
            revoked_account,
            format!("{prefix}-revoked@example.test"),
            "Cuenta Revocada".to_owned(),
            "disabled",
        ),
    ] {
        sqlx::query(
            "INSERT INTO commercial_account (id, email, display_name, status, created_at)
                 VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .bind(status)
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, 'owner', 1, $3)",
        )
        .bind(organization_id)
        .bind(first_account)
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at, revoked_at)
             VALUES ($1, $2, 'member', 1, $3, $4)",
        )
        .bind(organization_id)
        .bind(revoked_account)
        .bind(timestamp + time::Duration::seconds(1))
        .bind(timestamp + time::Duration::seconds(2))
        .execute(pool)
        .await
        .unwrap();
    for (status, association) in [("active", "active"), ("retired", "inactive")] {
        let product_id = Uuid::new_v4();
        let key = format!("{prefix}-product-{status}");
        sqlx::query(
                "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
                 VALUES ($1, $2, $3, '', $4, 'saas', now(), now())",
            )
            .bind(product_id)
            .bind(&key)
            .bind(format!("Product {key}"))
            .bind(status)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
                 VALUES ($1, $2, $3, now())",
        )
        .bind(organization_id)
        .bind(product_id)
        .bind(association)
        .execute(pool)
        .await
        .unwrap();
    }
    (prefix, organization_id)
}

#[tokio::test]
async fn control_directory_paginates_filters_and_preserves_history() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("staff-directory-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::ReadOnly,
    )
    .await;
    let (prefix, organization_id) = seed_control_fixture(&pool).await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = "http://127.0.0.1:8080";
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some(origin),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = staff_session_from(&headers);
    let audit_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_audit_event e
             JOIN staff_account a ON a.id = e.staff_account_id WHERE a.email = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations?search={prefix}&limit=2"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_rfc3339(&body["items"][0]["created_at"], "organization.created_at");
    let next_cursor = body["next_cursor"].as_str().unwrap().to_owned();

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations?search={prefix}&limit=2&cursor={next_cursor}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations?status=archived&search={prefix}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "archived")
    );

    for query in [
        "status=suspended".to_owned(),
        "cursor=no-es-un-cursor".to_owned(),
        format!("search={}", "x".repeat(201)),
    ] {
        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations?{query}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "query {query}");
        assert_eq!(body["code"], "validation_error");
    }

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations/{organization_id}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["memberships"].as_array().unwrap().len(), 2);
    for membership in body["memberships"].as_array().unwrap() {
        assert_rfc3339(&membership["joined_at"], "membership.joined_at");
        if !membership["revoked_at"].is_null() {
            assert_rfc3339(&membership["revoked_at"], "membership.revoked_at");
        }
    }
    assert!(
        body["memberships"]
            .as_array()
            .unwrap()
            .iter()
            .any(|membership| !membership["revoked_at"].is_null()),
        "la membership revocada se conserva"
    );
    for product in body["products"].as_array().unwrap() {
        assert_rfc3339(
            &product["activated_at"],
            "organization_product.activated_at",
        );
    }
    assert_eq!(body["products"].as_array().unwrap().len(), 2);
    assert!(
        body["products"]
            .as_array()
            .unwrap()
            .iter()
            .any(|product| product["association_status"] == "inactive")
    );

    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations/{}", Uuid::new_v4()),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/organizations/no-es-uuid",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/accounts?search={prefix}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_rfc3339(&body["items"][0]["created_at"], "account.created_at");
    let account_id = body["items"][0]["id"].as_str().unwrap().to_owned();

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/accounts/{account_id}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["memberships"].as_array().unwrap().len(), 1);
    assert_rfc3339(
        &body["memberships"][0]["joined_at"],
        "account_membership.joined_at",
    );

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/accounts?status=disabled&search={prefix}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    let audit_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_audit_event e
             JOIN staff_account a ON a.id = e.staff_account_id WHERE a.email = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_before, audit_after, "las lecturas no auditan");
}

#[tokio::test]
async fn control_directory_rejects_invalid_query_bounds() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("staff-bounds-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::ReadOnly,
    )
    .await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = staff_session_from(&headers);

    for path in ["/api/v1/control/organizations", "/api/v1/control/accounts"] {
        for query in ["limit=0", "limit=101", "limit=no-es-numero"] {
            let (status, _, body) = send_method(
                &router,
                Method::GET,
                &format!("{path}?{query}"),
                Some(&cookie),
                None,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?{query}");
            assert_eq!(body["code"], "validation_error", "{path}?{query}");
        }
    }
}

#[test]
fn control_directory_openapi_exposes_closed_enums() {
    let document = serde_json::to_value(openapi_document()).unwrap();
    for (schema, expected) in [
        ("OrganizationStatusDto", "legacy_unclaimed"),
        ("AccountStatusDto", "disabled"),
        ("MembershipRoleDto", "owner"),
        ("AssociationStatusDto", "inactive"),
    ] {
        let values = document["components"]["schemas"][schema]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("schema {schema}"));
        assert!(
            values.iter().any(|value| value == expected),
            "{schema} debe publicar {expected}"
        );
    }
}
