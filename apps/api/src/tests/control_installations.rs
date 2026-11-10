//! Pruebas de control_installations en el Control Center.

use super::common::*;
use super::*;

#[tokio::test]
async fn control_installations_require_staff_capability_and_cover_contract() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let router = build_router(state(pool.clone()), true, test_auth_state(pool.clone()));

    let organization = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at) \
             VALUES ($1, $2, 'active', now())",
    )
    .bind(organization)
    .bind(format!("API C5 {organization}"))
    .execute(&pool)
    .await
    .unwrap();
    let product = Uuid::new_v4();
    let product_key = format!("api-c5-{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO product \
             (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, $3, '', 'active', 'saas', now(), now())",
    )
    .bind(product)
    .bind(&product_key)
    .bind(format!("Product {product_key}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
             VALUES ($1, $2, 'active', now())",
    )
    .bind(organization)
    .bind(product)
    .execute(&pool)
    .await
    .unwrap();
    let owner = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_account (id, email, display_name, created_at) \
             VALUES ($1, $2, 'Owner API C5', now())",
    )
    .bind(owner)
    .bind(format!("api-c5-owner-{owner}@example.test"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership \
             (organization_id, account_id, role, role_version, joined_at) \
             VALUES ($1, $2, 'owner', 1, now())",
    )
    .bind(organization)
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();

    let installation = Uuid::new_v4();
    let device_label = format!("API C5 caja {}", &installation.to_string()[..8]);
    sqlx::query(
        "INSERT INTO installation \
             (id, organization_id, product_id, device_label, status, identity_status, \
              registered_at, enrolled_at) \
             VALUES ($1, $2, $3, $4, 'active', 'active', now(), now())",
    )
    .bind(installation)
    .bind(organization)
    .bind(product)
    .bind(&device_label)
    .execute(&pool)
    .await
    .unwrap();
    let identity = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installation_identity \
             (id, installation_id, public_key, fingerprint, generation, created_at) \
             VALUES ($1, $2, $3, $4, 1, now())",
    )
    .bind(identity)
    .bind(installation)
    .bind(random_bytes32())
    .bind(random_bytes32())
    .execute(&pool)
    .await
    .unwrap();

    let insert_request = |status: &'static str| {
        let consumed_at = (status == "completed").then(time::OffsetDateTime::now_utc);
        sqlx::query(
            "INSERT INTO enrollment_request \
                 (id, installation_id, product_id, public_key, fingerprint, device_label, \
                  app_version, platform, device_code_hash, user_code_hash, nonce_hash, status, \
                  organization_id, approved_by_account_id, idempotency_scope, idempotency_key, \
                  request_hash, created_at, expires_at, approved_at, consumed_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, '0.1.0', 'windows', $7, $8, $9, $10, \
                         $11, $12, 'api-c5', NULL, $13, now(), now() + interval '30 minutes', \
                         now(), $14)",
        )
        .bind(Uuid::new_v4())
        .bind(installation)
        .bind(product)
        .bind(random_bytes32())
        .bind(random_bytes32())
        .bind(format!("API C5 request {}", Uuid::new_v4()))
        .bind(random_bytes32())
        .bind(random_bytes32())
        .bind(random_bytes32())
        .bind(status)
        .bind(organization)
        .bind(owner)
        .bind(random_bytes32())
        .bind(consumed_at)
    };
    insert_request("completed").execute(&pool).await.unwrap();
    insert_request("approved").execute(&pool).await.unwrap();
    let original: Uuid = sqlx::query_scalar(
        "SELECT id FROM enrollment_request WHERE installation_id = $1 AND status = 'completed'",
    )
    .bind(installation)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO installation_binding \
             (id, organization_id, installation_id, enrollment_request_id, \
              installation_identity_id, approved_by_account_id, bound_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(Uuid::new_v4())
    .bind(organization)
    .bind(installation)
    .bind(original)
    .bind(identity)
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    let rehost_request: Uuid = sqlx::query_scalar(
        "SELECT id FROM enrollment_request WHERE installation_id = $1 AND status = 'approved'",
    )
    .bind(installation)
    .fetch_one(&pool)
    .await
    .unwrap();

    let password = "contraseña-staff-larga";
    let reader_email = format!("c5-reader-{}@hiqnet.test", Uuid::new_v4());
    let manager_email = format!("c5-manager-{}@hiqnet.test", Uuid::new_v4());
    let admin_email = format!("c5-admin-{}@hiqnet.test", Uuid::new_v4());
    seed_staff(
        &pool,
        &reader_email,
        password,
        platform_core::staff::StaffRole::Support,
    )
    .await;
    seed_staff(
        &pool,
        &manager_email,
        password,
        platform_core::staff::StaffRole::Operator,
    )
    .await;
    seed_staff(
        &pool,
        &admin_email,
        password,
        platform_core::staff::StaffRole::PlatformAdmin,
    )
    .await;

    let origin = Some("http://127.0.0.1:8080");
    let list_path =
        format!("/api/v1/control/installations?organization_id={organization}&limit=10");
    let (status, _, _) = send_method(&router, Method::GET, &list_path, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "sin sesión staff");

    let mut dev_request = Request::builder()
        .method(Method::GET)
        .uri(&list_path)
        .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
        .body(Body::empty())
        .unwrap();
    dev_request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            40000,
        ))));
    let response = router.clone().oneshot(dev_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "devAdminToken no accede a Control"
    );

    let reader = control_login(&router, &reader_email, password).await;
    let manager = control_login(&router, &manager_email, password).await;
    let admin = control_login(&router, &admin_email, password).await;

    let (status, _, body) =
        send_method(&router, Method::GET, &list_path, Some(&reader), None, None).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["id"] == json!(installation)));
    let registered_at = items[0]["registered_at"].as_str().unwrap();
    assert!(
        time::OffsetDateTime::parse(
            registered_at,
            &time::format_description::well_known::Rfc3339
        )
        .is_ok(),
        "registered_at debe ser RFC3339: {registered_at}"
    );

    let (status, _, detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/installations/{installation}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["summary"]["id"], json!(installation));
    assert_eq!(detail["summary"]["product"]["key"], json!(product_key));
    assert_eq!(
        detail["current_binding"]["approved_by_account_id"],
        json!(owner)
    );
    let identity_created_at = detail["current_identity_created_at"]
        .as_str()
        .expect("current_identity_created_at debe ser RFC3339");
    time::OffsetDateTime::parse(
        identity_created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .expect("current_identity_created_at no parsea como RFC3339");

    for (path, expected_status, expected_code) in [
        (
            format!("/api/v1/control/installations/{}", Uuid::new_v4()),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            "/api/v1/control/installations/no-es-uuid".into(),
            StatusCode::BAD_REQUEST,
            "invalid_path",
        ),
    ] {
        let (status, _, error) =
            send_method(&router, Method::GET, &path, Some(&reader), None, None).await;
        assert_eq!(status, expected_status, "{path}");
        assert_eq!(error["code"], expected_code);
    }

    for query in ["limit=0", "limit=101", "limit=abc", "cursor=no-es-cursor"] {
        let (status, _, error) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/installations?{query}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(error["code"], "validation_error", "{query}");
    }
    for query in ["status=bogus", "identity_status=bogus"] {
        let (status, _, error) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/installations?{query}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(error["code"], "validation_error", "{query}");
    }
    let (status, _, error) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/installations?search={}", "x".repeat(201)),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "validation_error");

    let suspend_path = format!("/api/v1/control/installations/{installation}/suspend");
    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &suspend_path,
        Some(&reader),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "read sin manage");

    let (status, _, suspended) = send_method(
        &router,
        Method::POST,
        &suspend_path,
        Some(&manager),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        suspended["summary"]["administrative_status"],
        json!("suspended")
    );
    let (status, _, reactivated) = send_method(
        &router,
        Method::POST,
        &format!("/api/v1/control/installations/{installation}/reactivate"),
        Some(&manager),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        reactivated["summary"]["administrative_status"],
        json!("active")
    );

    let rehost_path = format!("/api/v1/control/installations/{installation}/rehost");
    let rehost_body = json!({"enrollment_request_id": rehost_request, "reason": "reemplazo"});
    let (status, _, error) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&manager),
        origin,
        Some(rehost_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "manage sin rehost");
    assert_eq!(error["code"], "forbidden");

    let (status, _, error) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&admin),
        Some("https://evil.example"),
        Some(rehost_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["code"], "origin_not_allowed");

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&admin),
        origin,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, error) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&admin),
        origin,
        Some(json!({"enrollment_request_id": rehost_request, "reason": "   "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "validation_error");

    let (status, _, error) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&admin),
        origin,
        Some(json!({"enrollment_request_id": Uuid::new_v4(), "reason": "ajena"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "rehost_conflict");

    let (status, _, authorization) = send_method(
        &router,
        Method::POST,
        &rehost_path,
        Some(&admin),
        origin,
        Some(rehost_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(authorization["status"], json!("pending"));
    assert_eq!(
        authorization["enrollment_request_id"],
        json!(rehost_request)
    );

    let (status, _, detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/installations/{installation}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["summary"]["identity_status"], json!("unenrolled"));
    assert_eq!(detail["summary"]["binding_active"], json!(false));
    assert!(detail["current_binding"].is_null());
    assert!(detail["current_identity_created_at"].is_null());
    assert_eq!(detail["rehost"]["status"], json!("pending"));
}
