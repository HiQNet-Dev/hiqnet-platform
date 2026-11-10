use super::common::*;
use super::*;

#[tokio::test]
async fn login_me_logout_and_csrf_match_contract() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("api-{}@example.test", Uuid::new_v4());
    let password = "contraseña-api-larga";
    seed_credential(&pool, &email, password).await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let login_body = json!({"email": email, "password": password});

    // Origen ajeno: rechazado antes de tocar credenciales.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("https://evil.example"),
        Some(login_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(headers.get(header::SET_COOKIE).is_none());

    // Contraseña incorrecta: error genérico y sin cookie.
    let (status, headers, body) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": "incorrecta"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "invalid_credentials");
    assert!(headers.get(header::SET_COOKIE).is_none());

    // Login correcto: cookie de sesión opaca.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(login_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = session_from(&headers);
    assert!(!cookie.contains("Secure"), "DEV sin HTTPS no marca Secure");

    // /me deriva la identidad de la cookie, no del cuerpo.
    let me = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let bytes = to_bytes(me.into_body(), 1024 * 1024).await.unwrap();
    let me_body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me_body["email"], email);

    // Cookie alterada no autentica.
    let tampered = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, "hiqnet_session=alterado")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tampered.status(), StatusCode::UNAUTHORIZED);

    // Logout real: revoca y borra la cookie.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/logout",
        Some(&cookie),
        Some("http://127.0.0.1:8080"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        headers
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let after = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);

    // Logout con origen ajeno no revoca ni toca la sesión.
    let (status, _, _) = send_auth(
        &router,
        "/api/v1/auth/logout",
        Some(&cookie),
        Some("https://evil.example"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn production_login_requires_exact_origin_and_rejects_null_or_malformed() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let mut auth_state = test_auth_state(pool.clone());
    auth_state.development = false;
    let router = build_router(state(pool.clone()), false, auth_state);
    let body = || json!({"email":"nadie@example.test","password":"x"});

    let (status, _, _) = send_auth(&router, "/api/v1/auth/login", None, None, Some(body())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "sin Origin en producción");

    for origin in ["null", "https://evil.example", "http://127.0.0.1:8080/"] {
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            Some(origin),
            Some(body()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "origen {origin}");
    }

    let duplicate = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, "http://127.0.0.1:8080")
                .header(header::ORIGIN, "http://127.0.0.1:8080")
                .body(Body::from(body().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::FORBIDDEN);

    let malformed = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    header::ORIGIN,
                    axum::http::HeaderValue::from_bytes(b"\xFF").unwrap(),
                )
                .body(Body::from(body().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::FORBIDDEN);

    // Origen exacto: llega a la verificación y responde credenciales inválidas.
    let (status, _, response) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(body()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(response["code"], "invalid_credentials");
}

#[tokio::test]
async fn argon2_saturation_returns_controlled_503_and_recovers() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let mut auth_state = test_auth_state(pool.clone());
    auth_state.argon2_slots = Arc::new(tokio::sync::Semaphore::new(1));
    let held = auth_state.argon2_slots.clone().try_acquire_owned().unwrap();
    let router = build_router(state(pool.clone()), false, auth_state);
    let body = json!({"email":"nadie@example.test","password":"x"});
    let (status, _, response) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response["code"], "auth_overloaded");

    drop(held);
    let (status, _, response) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(response["code"], "invalid_credentials");
}

#[tokio::test]
async fn auth_context_distinguishes_invalid_session_from_storage_outage() {
    let unavailable = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
        .unwrap();
    let router = build_router(
        state(unavailable.clone()),
        false,
        test_auth_state(unavailable),
    );
    let outage = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, "hiqnet_session=token-sintetico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(outage.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = to_bytes(outage.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "storage_error");
    assert!(!body.to_string().contains("postgres://"));
    assert!(!body.to_string().contains("SELECT"));

    let organizations_outage = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations")
                .header(header::COOKIE, "hiqnet_session=token-sintetico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        organizations_outage.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );

    let members_outage = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/organizations/{}/members", Uuid::new_v4()))
                .header(header::COOKIE, "hiqnet_session=token-sintetico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(members_outage.status(), StatusCode::SERVICE_UNAVAILABLE);

    let pool = open(&isolated_test_url()).await.unwrap();
    let router = build_router(state(pool.clone()), false, test_auth_state(pool));
    let invalid = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, "hiqnet_session=token-sintetico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(invalid.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn password_change_rotates_cookie_and_revokes_previous_session() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("pwc-{}@example.test", Uuid::new_v4());
    let initial = "contraseña-inicial-1";
    let new_password = "nueva-clave-segura-2026";
    seed_credential(&pool, &email, initial).await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = Some("http://127.0.0.1:8080");

    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": email, "password": initial})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let old_cookie = session_from(&headers);

    let (status, _, body) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&old_cookie),
        origin,
        Some(json!({"current_password": "incorrecta", "new_password": new_password})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "invalid_credentials");

    let (status, _, body) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&old_cookie),
        origin,
        Some(json!({"current_password": initial, "new_password": "corta1"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "weak_password");

    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&old_cookie),
        origin,
        Some(json!({"current_password": initial, "new_password": new_password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let new_cookie = session_from(&headers);
    assert_ne!(old_cookie, new_cookie);

    let old_me = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &old_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_me.status(), StatusCode::UNAUTHORIZED);
    let new_me = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &new_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_me.status(), StatusCode::OK);

    let (status, _, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": email, "password": initial})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "hash anterior invalidado");
    let (status, _, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": email, "password": new_password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn session_revoke_requires_origin_and_invalidates_current_cookie() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("revoke-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email, password).await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = Some("http://127.0.0.1:8080");

    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = session_from(&headers);

    let (status, _, _) = send_auth(
        &router,
        "/api/v1/auth/sessions/revoke",
        Some(&cookie),
        Some("https://evil.example"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/sessions/revoke",
        Some(&cookie),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        headers
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );

    let after = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);

    let (status, _, _) =
        send_auth(&router, "/api/v1/auth/sessions/revoke", None, origin, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Una cookie revocada no puede completar un cambio de contraseña.
    let (status, _, body) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&cookie),
        origin,
        Some(json!({"current_password": password, "new_password": "nueva-clave-segura-2026"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn password_change_is_rate_limited() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("pwrate-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email, password).await;
    let mut auth_state = test_auth_state(pool.clone());
    auth_state.limiter = Arc::new(auth::LoginLimiter::new(
        1,
        std::time::Duration::from_secs(60),
    ));
    let router = build_router(state(pool.clone()), false, auth_state);
    let origin = Some("http://127.0.0.1:8080");
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = session_from(&headers);
    let body = json!({"current_password": "incorrecta", "new_password": "nueva-clave-segura-2026"});
    let (status, _, _) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&cookie),
        origin,
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, response) = send_auth(
        &router,
        "/api/v1/auth/password/change",
        Some(&cookie),
        origin,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response["code"], "rate_limited");
}

#[tokio::test]
async fn login_rate_limit_blocks_repeated_attempts() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("rate-{}@example.test", Uuid::new_v4());
    seed_credential(&pool, &email, "contraseña-api-larga").await;
    let mut auth_state = test_auth_state(pool.clone());
    auth_state.limiter = Arc::new(auth::LoginLimiter::new(
        2,
        std::time::Duration::from_secs(60),
    ));
    let router = build_router(state(pool.clone()), false, auth_state);
    let bad = json!({"email": email, "password": "incorrecta"});
    for _ in 0..2 {
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            Some("http://127.0.0.1:8080"),
            Some(bad.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, body) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(bad),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["code"], "rate_limited");
}

#[tokio::test]
async fn staff_auth_is_separate_and_revocable() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("staff-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::SuperAdmin,
    )
    .await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));

    // Origen ajeno rechazado antes de tocar credenciales.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some("https://evil.example"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(headers.get(header::SET_COOKIE).is_none());

    // Login staff correcto: cookie propia, SameSite=Strict, Path del Control Center.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let staff_cookie = staff_session_from(&headers);

    // /me deriva identidad y capabilities de la sesión verificada.
    let (status, _, body) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/auth/me",
        Some(&staff_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "super_admin");
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability.as_str() == Some("staff.manage"))
    );

    // El token DEV no concede acceso staff.
    let mut request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/control/auth/me")
        .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            40000,
        ))));
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Una cookie comercial no accede al Control Center.
    let commercial_email = format!("api-{}@example.test", Uuid::new_v4());
    seed_credential(&pool, &commercial_email, "contraseña-api-larga").await;
    let (status, commercial_headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": commercial_email, "password": "contraseña-api-larga"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let commercial_cookie = session_from(&commercial_headers);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/auth/me",
        Some(&commercial_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Logout revoca la sesión staff.
    let (status, _, _) = send_auth(
        &router,
        "/api/v1/control/auth/logout",
        Some(&staff_cookie),
        Some("http://127.0.0.1:8080"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/auth/me",
        Some(&staff_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Deshabilitar la cuenta invalida de inmediato cualquier sesión nueva.
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let active_cookie = staff_session_from(&headers);
    sqlx::query("UPDATE staff_account SET status = 'disabled' WHERE email = $1")
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/auth/me",
        Some(&active_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
