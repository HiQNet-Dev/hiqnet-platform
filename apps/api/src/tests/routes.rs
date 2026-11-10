use super::common::*;
use super::*;

#[test]
fn generated_contract_matches_versioned_snapshot_and_expected_routes() {
    let current = serde_json::to_value(openapi_document()).unwrap();
    let saved: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../openapi/openapi.json"
    )))
    .unwrap();
    assert_eq!(
        current, saved,
        "regenera con cargo run -p hiqnet-platform-api --bin generate-openapi"
    );
    assert_eq!(current["openapi"], "3.1.0");
    let expected = [
        ("/health", "get"),
        ("/ready", "get"),
        ("/api/v1/plans", "get"),
        ("/api/v1/installations", "post"),
        ("/api/v1/installations/{id}", "get"),
        ("/api/v1/installations/{id}/trial", "post"),
        ("/api/v1/installations/{id}/licenses", "get"),
        ("/api/v1/installations/{id}/licenses", "post"),
        ("/api/v1/auth/login", "post"),
        ("/api/v1/auth/logout", "post"),
        ("/api/v1/auth/me", "get"),
        ("/api/v1/auth/password/change", "post"),
        ("/api/v1/auth/sessions/revoke", "post"),
        ("/api/v1/organizations", "get"),
        ("/api/v1/organizations", "post"),
        ("/api/v1/organizations/{id}", "get"),
        ("/api/v1/organizations/{id}/members", "get"),
        ("/api/v1/organizations/{id}/members", "post"),
        ("/api/v1/organizations/{id}/members/{account_id}", "patch"),
        ("/api/v1/organizations/{id}/members/{account_id}", "delete"),
        ("/api/v1/enrollment-requests", "post"),
        ("/api/v1/enrollment-requests/{user_code}/approve", "post"),
        ("/api/v1/enrollment-requests/{device_code}/proof", "post"),
        ("/api/v1/organizations/{id}/installations", "get"),
        ("/api/v1/control/auth/login", "post"),
        ("/api/v1/control/auth/logout", "post"),
        ("/api/v1/control/auth/me", "get"),
        ("/api/v1/control/products", "get"),
        ("/api/v1/control/products", "post"),
        ("/api/v1/control/products/{key}", "get"),
        ("/api/v1/control/products/{key}", "patch"),
        ("/api/v1/control/projects", "get"),
        ("/api/v1/control/projects", "post"),
        ("/api/v1/control/projects/{key}", "get"),
        ("/api/v1/control/projects/{key}", "patch"),
        ("/api/v1/control/organizations", "get"),
        ("/api/v1/control/organizations/{organization_id}", "get"),
        ("/api/v1/control/accounts", "get"),
        ("/api/v1/control/accounts/{account_id}", "get"),
        ("/api/v1/control/installations", "get"),
        ("/api/v1/control/installations/{installation_id}", "get"),
        (
            "/api/v1/control/installations/{installation_id}/suspend",
            "post",
        ),
        (
            "/api/v1/control/installations/{installation_id}/reactivate",
            "post",
        ),
        (
            "/api/v1/control/installations/{installation_id}/rehost",
            "post",
        ),
        ("/api/v1/control/audit", "get"),
        ("/api/v1/control/audit/{event_id}", "get"),
        ("/api/v1/control/security-events", "get"),
        ("/api/v1/control/security-events/{event_id}", "get"),
        ("/api/v1/control/staff-auth-events", "get"),
        ("/api/v1/control/staff-auth-events/{event_id}", "get"),
        (
            "/api/v1/control/organizations/{organization_id}/products",
            "get",
        ),
        (
            "/api/v1/control/organizations/{organization_id}/products",
            "post",
        ),
        (
            "/api/v1/control/organizations/{organization_id}/products/{product_id}",
            "patch",
        ),
        ("/api/v1/control/plans", "get"),
        ("/api/v1/control/plans", "post"),
        ("/api/v1/control/plans/{plan_id}", "get"),
        ("/api/v1/control/plans/{plan_id}", "patch"),
        ("/api/v1/control/subscriptions", "get"),
        ("/api/v1/control/subscriptions", "post"),
        ("/api/v1/control/subscriptions/{subscription_id}", "get"),
        (
            "/api/v1/control/subscriptions/{subscription_id}/suspend",
            "post",
        ),
        (
            "/api/v1/control/subscriptions/{subscription_id}/reactivate",
            "post",
        ),
        (
            "/api/v1/control/subscriptions/{subscription_id}/cancel",
            "post",
        ),
        (
            "/api/v1/control/subscriptions/{subscription_id}/renew",
            "post",
        ),
        (
            "/api/v1/control/organizations/{organization_id}/entitlements",
            "get",
        ),
        ("/api/v1/organizations/{organization_id}/products", "get"),
        (
            "/api/v1/organizations/{organization_id}/products/{product_id}",
            "get",
        ),
    ];
    let actual: Vec<(&str, &str)> = current["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, operations)| {
            operations
                .as_object()
                .unwrap()
                .keys()
                .map(move |method| (path.as_str(), method.as_str()))
        })
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (path, method) in expected {
        let operation = &current["paths"][path][method];
        assert!(operation.is_object(), "ruta {method} {path} ausente");
        assert!(operation["operationId"].is_string());
        assert!(operation["summary"].is_string());
        assert!(
            operation["responses"]
                .as_object()
                .is_some_and(|responses| !responses.is_empty())
        );
        if path.starts_with("/api/v1/installations") {
            assert_eq!(operation["security"], json!([{"devAdminToken": []}]));
        } else if path == "/api/v1/control/auth/login" {
            assert!(operation.get("security").is_none());
        } else if path.starts_with("/api/v1/control/") {
            assert_eq!(operation["security"], json!([{"staffSession": []}]));
        } else if path == "/api/v1/auth/login"
            || path == "/api/v1/enrollment-requests"
            || path.ends_with("/proof")
        {
            assert!(operation.get("security").is_none());
        } else if matches!(
            path,
            "/api/v1/auth/logout"
                | "/api/v1/auth/me"
                | "/api/v1/auth/password/change"
                | "/api/v1/auth/sessions/revoke"
                | "/api/v1/enrollment-requests/{user_code}/approve"
        ) || path.starts_with("/api/v1/organizations")
        {
            assert_eq!(operation["security"], json!([{"commercialSession": []}]));
        } else {
            assert!(operation.get("security").is_none());
        }
    }
}

#[tokio::test]
async fn swagger_is_opt_in_and_serves_generated_document() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
        .unwrap();
    let auth_state = test_auth_state(pool.clone());
    let disabled = build_router(state(pool.clone()), false, auth_state.clone());
    assert_eq!(
        send(&disabled, Method::GET, "/docs", None, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&disabled, Method::GET, "/api-docs/openapi.json", None, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let enabled = build_router(state(pool), true, auth_state);
    let spec = send(&enabled, Method::GET, "/api-docs/openapi.json", None, None).await;
    assert_eq!(spec.0, StatusCode::OK);
    assert_eq!(spec.1, serde_json::to_value(openapi_document()).unwrap());
    let redirect = enabled
        .clone()
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(redirect.status().is_redirection());
    assert_eq!(redirect.headers()[header::LOCATION], "/docs/");
    let ui = enabled
        .oneshot(
            Request::builder()
                .uri("/docs/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ui.status(), StatusCode::OK);
    let html = to_bytes(ui.into_body(), 1024 * 1024).await.unwrap();
    assert!(String::from_utf8_lossy(&html).contains("Swagger UI"));
}

#[tokio::test]
async fn http_routes_auth_errors_and_real_postgres_match_contract() {
    let url = isolated_test_url();
    let pool = open(&url).await.unwrap();
    let router = build_router(
        dev_state(pool.clone(), false),
        true,
        test_auth_state(pool.clone()),
    );
    let id = Uuid::new_v4();
    let org = Uuid::new_v4();
    test_organization(&pool, org).await;
    let path = format!("/api/v1/installations/{id}");
    let body =
        json!({"installation_id":id,"organization_id":org,"device_label":format!("fixture-{id}")});
    let token = Some("token-sintetico-p01");

    assert_eq!(
        send(&router, Method::GET, "/health", None, None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&router, Method::GET, "/ready", None, None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&router, Method::GET, "/api/v1/plans", None, None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        send(&router, Method::GET, &path, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&router, Method::POST, &format!("{path}/trial"), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &router,
            Method::POST,
            &format!("{path}/licenses"),
            None,
            Some(json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&router, Method::GET, &path, Some("incorrecto"), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            None,
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );

    let registered = send(
        &router,
        Method::POST,
        "/api/v1/installations",
        token,
        Some(body.clone()),
    )
    .await;
    assert_eq!(registered.0, StatusCode::CREATED);
    assert_eq!(registered.1["id"], id.to_string());
    assert_eq!(
        send(&router, Method::GET, &path, token, None).await.0,
        StatusCode::OK
    );
    let missing = send(
        &router,
        Method::GET,
        &format!("/api/v1/installations/{}", Uuid::new_v4()),
        token,
        None,
    )
    .await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert_eq!(missing.1["code"], "not_found");
    assert_eq!(
        send(
            &router,
            Method::GET,
            "/api/v1/installations/bad-uuid",
            token,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &router,
            Method::GET,
            &format!("{path}/licenses"),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &router,
            Method::GET,
            &format!("{path}/licenses"),
            token,
            None
        )
        .await
        .1,
        json!([])
    );

    let trial = send(&router, Method::POST, &format!("{path}/trial"), token, None).await;
    assert_eq!(trial.0, StatusCode::OK);
    assert!(trial.1["trial_ends_at"].is_string());
    assert_eq!(
        send(&router, Method::POST, &format!("{path}/trial"), token, None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let duplicate = json!({"installation_id":Uuid::new_v4(),"organization_id":org,"device_label":format!("fixture-{id}")});
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(duplicate)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let other_org = Uuid::new_v4();
    test_organization(&pool, other_org).await;
    let wrong_org = json!({"installation_id":id,"organization_id":other_org,"device_label":format!("fixture-{id}")});
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(wrong_org)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    remove_test_organization(&pool, other_org).await;
    let invalid = json!({"organization_id":org,"device_label":"   "});
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(invalid)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let invalid_type = json!({"organization_id":org,"device_label":99});
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(invalid_type)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let issue = json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"});
    assert_eq!(
        send(
            &router,
            Method::POST,
            &format!("{path}/licenses"),
            token,
            Some(issue)
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );

    // La fixture sólo existe en TEST_DATABASE_URL; no altera la base principal.
    // Desde 0009 el historial de instalaciones es inmutable: no se limpia con DELETE.
    assert!(
        sqlx::query("DELETE FROM installation WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .is_err(),
        "la instalación no admite eliminación física"
    );
}

#[tokio::test]
async fn liveness_does_not_claim_database_health_when_database_is_down() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
        .unwrap();
    assert_eq!(
        crate::health::health().await.into_response().status(),
        StatusCode::OK
    );
    assert_eq!(
        crate::health::ready(State(state(pool)))
            .await
            .into_response()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn readiness_uses_real_postgres() {
    let url = isolated_test_url();
    let pool = open(&url).await.unwrap();
    assert_eq!(
        crate::health::ready(State(state(pool)))
            .await
            .into_response()
            .status(),
        StatusCode::OK
    );
}
