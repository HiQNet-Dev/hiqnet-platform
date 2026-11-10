use super::common::*;
use super::*;

#[tokio::test]
async fn dev_signing_and_json_rejections_match_documented_responses() {
    let url = isolated_test_url();
    let pool = open(&url).await.unwrap();
    let router = build_router(
        dev_state(pool.clone(), true),
        false,
        test_auth_state(pool.clone()),
    );
    let id = Uuid::new_v4();
    let org = Uuid::new_v4();
    test_organization(&pool, org).await;
    let token = Some("token-sintetico-p01");
    let path = format!("/api/v1/installations/{id}/licenses");
    let registration = json!({"installation_id": id, "organization_id": org, "device_label":format!("fixture-{id}")});
    assert_eq!(
        send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(registration)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        send(
            &router,
            Method::POST,
            &path,
            token,
            Some(json!({"plan":"unknown", "valid_until":"2030-01-01T00:00:00Z"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &router,
            Method::POST,
            &path,
            token,
            Some(json!({"plan":"trial", "valid_until":"2000-01-01T00:00:00Z"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let issued = send(
        &router,
        Method::POST,
        &path,
        token,
        Some(json!({"plan":"trial", "valid_until":"2030-01-01T00:00:00Z"})),
    )
    .await;
    assert_eq!(issued.0, StatusCode::CREATED);
    assert!(issued.1["grant_json"].is_string());
    assert!(issued.1["signature_b64"].is_string());
    let list = send(&router, Method::GET, &path, token, None).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(list.1[0]["grant_id"], issued.1["grant_id"]);
    assert!(list.1[0]["features_json"].is_string());

    let absent_type = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/installations")
                .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(absent_type.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let malformed = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/installations")
                .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert!(
        malformed
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json")
    );
    let cross_origin = router
        .oneshot(
            Request::builder()
                .uri("/health")
                .header(header::ORIGIN, "https://otro.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        cross_origin
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );

    sqlx::query("DELETE FROM issued_license WHERE installation_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
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
