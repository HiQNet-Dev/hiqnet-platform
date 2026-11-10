use super::common::*;
use super::*;

#[tokio::test]
async fn commercial_plans_subscriptions_and_entitlements_cover_contract() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let admin_email = format!("commercial-admin-{}@hiqnet.test", Uuid::new_v4());
    let reader_email = format!("commercial-reader-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &admin_email,
        password,
        platform_core::staff::StaffRole::PlatformAdmin,
    )
    .await;
    seed_staff(
        &pool,
        &reader_email,
        password,
        platform_core::staff::StaffRole::ReadOnly,
    )
    .await;
    let organization = Uuid::new_v4();
    let product = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Commercial API org', 'active', now())",
    )
    .bind(organization)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, 'Commercial API product', '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(format!("commercial-api-{}", Uuid::new_v4().simple()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, 'active', now())",
    )
    .bind(organization)
    .bind(product)
    .execute(&pool)
    .await
    .unwrap();

    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = Some("http://127.0.0.1:8080");
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        origin,
        Some(json!({"email": admin_email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let admin = staff_session_from(&headers);
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        origin,
        Some(json!({"email": reader_email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let reader = staff_session_from(&headers);

    let (status, _, plan) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/plans",
        Some(&admin),
        origin,
        Some(json!({
            "product_id": product,
            "key": "pro",
            "name": "Pro",
            "status": "active",
            "features": {"sync": true},
            "limits": {"seats": 10}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let plan_id: Uuid = serde_json::from_value(plan["id"].clone()).unwrap();

    let (status, _, plans) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/plans?product_id={product}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plans["items"].as_array().unwrap().len(), 1);

    let subscription_body = json!({
        "organization_id": organization,
        "product_id": product,
        "plan_id": plan_id,
        "starts_at": "2026-01-01T00:00:00Z",
        "ends_at": "2030-01-01T00:00:00Z"
    });
    let idempotency = format!("create-{}", Uuid::new_v4());
    let create_request = |body: Value| {
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/control/subscriptions")
            .header(header::COOKIE, &admin)
            .header(header::ORIGIN, "http://127.0.0.1:8080")
            .header("Idempotency-Key", &idempotency)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let response = router
        .clone()
        .oneshot(create_request(subscription_body.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let created: Value = serde_json::from_slice(&bytes).unwrap();
    let subscription_id: Uuid = serde_json::from_value(created["id"].clone()).unwrap();

    let response = router
        .clone()
        .oneshot(create_request(subscription_body.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let repeated: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(repeated["id"], json!(subscription_id));

    let mut conflicting = subscription_body;
    conflicting["ends_at"] = json!("2031-01-01T00:00:00Z");
    let response = router
        .clone()
        .oneshot(create_request(conflicting))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let conflict: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(conflict["code"], json!("idempotency_conflict"));

    let (status, _, entitlements) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations/{organization}/entitlements?product_id={product}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entitlements["effective"], json!(true));
    assert_eq!(entitlements["features"]["sync"], json!(true));

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &format!("/api/v1/control/subscriptions/{subscription_id}/suspend"),
        Some(&admin),
        origin,
        Some(json!({"reason": "review"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, suspended) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations/{organization}/entitlements?product_id={product}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(suspended["effective"], json!(false));
    assert_eq!(suspended["reason"], json!("suspended"));

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &format!("/api/v1/control/subscriptions/{subscription_id}/reactivate"),
        Some(&admin),
        origin,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, new_plan) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/plans",
        Some(&admin),
        origin,
        Some(json!({
            "product_id": product,
            "key": "pro-2",
            "name": "Pro 2",
            "status": "active",
            "limits": {"seats": 20}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let new_plan_id: Uuid = serde_json::from_value(new_plan["id"].clone()).unwrap();

    let renew_request = Request::builder()
        .method(Method::POST)
        .uri(format!(
            "/api/v1/control/subscriptions/{subscription_id}/renew"
        ))
        .header(header::COOKIE, &admin)
        .header(header::ORIGIN, "http://127.0.0.1:8080")
        .header("Idempotency-Key", format!("renew-{}", Uuid::new_v4()))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"plan_id": new_plan_id, "ends_at": "2031-01-01T00:00:00Z"}).to_string(),
        ))
        .unwrap();
    let response = router.clone().oneshot(renew_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let renewed: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        renewed["renewed_from_subscription_id"],
        json!(subscription_id)
    );

    let (status, _, old) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/subscriptions/{subscription_id}"),
        Some(&reader),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old["subscription"]["status"], json!("cancelled"));
    assert!(
        old["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["event"] == json!("cancelled"))
    );

    let (status, _, _) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/plans/{plan_id}"),
        Some(&admin),
        origin,
        Some(json!({"features": {"sync": false}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/plans",
        Some(&reader),
        origin,
        Some(json!({
            "product_id": product,
            "key": "reader-plan",
            "name": "Reader plan"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/subscriptions",
        Some(&admin),
        origin,
        Some(json!({
            "organization_id": organization,
            "product_id": product,
            "plan_id": new_plan_id,
            "starts_at": "2026-01-01T00:00:00Z",
            "ends_at": "2030-01-01T00:00:00Z"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn portal_commercial_products_are_read_only_and_tenant_isolated() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("portal-commercial-{}@example.test", Uuid::new_v4());
    let password = "contraseña-api-larga";
    seed_credential(&pool, &email, password).await;
    let account: Uuid = sqlx::query_scalar("SELECT id FROM commercial_account WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    let organization = Uuid::new_v4();
    let other_organization = Uuid::new_v4();
    let product = Uuid::new_v4();
    let plan = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Portal commercial org', 'active', now()),
                    ($2, 'Other org', 'active', now())",
    )
    .bind(organization)
    .bind(other_organization)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, 'Portal product', '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(format!("portal-{}", Uuid::new_v4().simple()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, 'active', now())",
    )
    .bind(organization)
    .bind(product)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, 'owner', 1, now())",
        )
        .bind(organization)
        .bind(account)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO plan (id, product_id, key, name, description, status, is_trial, created_at, updated_at)
             VALUES ($1, $2, 'portal-plan', 'Portal plan', '', 'active', false, now(), now())",
        )
        .bind(plan)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO subscription
             (id, organization_id, product_id, plan_id, status, is_trial, starts_at, ends_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'active', false, now() - interval '1 day', now() + interval '30 days', now(), now())",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(product)
        .bind(plan)
        .execute(&pool)
        .await
        .unwrap();

    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = session_from(&headers);

    let (status, _, products) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization}/products"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(products["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        products["items"][0]["entitlements"]["effective"],
        json!(true)
    );
    assert_eq!(
        products["items"][0]["subscription"]["plan"]["key"],
        json!("portal-plan")
    );

    let (status, _, product_detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization}/products/{product}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(product_detail["product"]["id"], json!(product));

    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{other_organization}/products"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization}/products"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
