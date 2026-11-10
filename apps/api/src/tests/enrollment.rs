use super::common::*;
use super::*;

#[tokio::test]
async fn enrollment_rate_limiters_are_scoped_by_peer_and_account() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email_a = format!("rate-a-{}@example.test", Uuid::new_v4());
    let email_b = format!("rate-b-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email_a, password).await;
    seed_credential(&pool, &email_b, password).await;
    let mut app_state = state(pool.clone());
    app_state.enrollment_begin_limiter = Arc::new(auth::LoginLimiter::new(
        1,
        std::time::Duration::from_secs(60),
    ));
    app_state.enrollment_proof_limiter = Arc::new(auth::LoginLimiter::new(
        1,
        std::time::Duration::from_secs(60),
    ));
    app_state.enrollment_approve_limiter = Arc::new(auth::LoginLimiter::new(
        1,
        std::time::Duration::from_secs(60),
    ));
    let router = build_router(app_state, false, test_auth_state(pool.clone()));
    let product_key = format!("rate-pos-{}", Uuid::new_v4().simple());
    sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, 'Rate product', '', 'active', 'desktop', now(), now())",
        )
        .bind(Uuid::new_v4())
        .bind(&product_key)
        .execute(&pool)
        .await
        .unwrap();
    let peer_a = std::net::SocketAddr::from(([127, 0, 0, 2], 41001));
    let peer_b = std::net::SocketAddr::from(([127, 0, 0, 3], 41002));
    let origin = Some("http://127.0.0.1:8080");
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    let public_key = format!(
        "ed25519:v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing_key.verifying_key().as_bytes())
    );
    let body = |label: &str| {
        json!({
            "installation_id": Uuid::new_v4(),
            "product_key": product_key,
            "public_key": public_key.clone(),
            "device_label": label,
            "app_version": "0.1.0",
            "platform": "linux"
        })
    };
    let (status, _, _) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests",
        None,
        None,
        Some(body("Caja rate A")),
        peer_a,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _, response) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests",
        None,
        None,
        Some(body("Caja rate A2")),
        peer_a,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response["code"], "rate_limited");
    let (status, _, _) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests",
        None,
        None,
        Some(body("Caja rate B")),
        peer_b,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let proof_body = json!({
        "installation_id": Uuid::new_v4(),
        "public_key": public_key,
        "nonce": "nonce-rate",
        "signature": "firma-rate"
    });
    let (status, _, _) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/device-rate/proof",
        None,
        None,
        Some(proof_body.clone()),
        peer_a,
    )
    .await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/device-rate/proof",
        None,
        None,
        Some(proof_body.clone()),
        peer_a,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = send_method_from(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/device-rate/proof",
        None,
        None,
        Some(proof_body),
        peer_b,
    )
    .await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);

    let login = |email: String| {
        let router = router.clone();
        async move {
            let (status, headers, _) = send_auth(
                &router,
                "/api/v1/auth/login",
                None,
                Some("http://127.0.0.1:8080"),
                Some(json!({"email": email, "password": password})),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            session_from(&headers)
        }
    };
    let cookie_a = login(email_a).await;
    let cookie_b = login(email_b).await;
    let approve_body = json!({"organization_id": Uuid::new_v4()});
    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/AAAA-BBBB/approve",
        Some(&cookie_a),
        origin,
        Some(approve_body.clone()),
    )
    .await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/AAAA-BBBB/approve",
        Some(&cookie_a),
        origin,
        Some(approve_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests/AAAA-BBBB/approve",
        Some(&cookie_b),
        origin,
        Some(approve_body),
    )
    .await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn enrollment_http_flow_rbac_and_cross_tenant() {
    use ed25519_dalek::Signer as _;
    use platform_core::enrollment::canonical_proof_message;

    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("enroll-{}@example.test", Uuid::new_v4());
    let other_email = format!("other-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email, password).await;
    seed_credential(&pool, &other_email, password).await;
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
    let owner_cookie = session_from(&headers);

    let (status, _, organization) = send_method(
        &router,
        Method::POST,
        "/api/v1/organizations",
        Some(&owner_cookie),
        origin,
        Some(json!({"display_name": "Org enrollment HTTP"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let organization_id = organization["id"].as_str().unwrap().to_owned();
    let product_key = format!("http-pos-{}", Uuid::new_v4().simple());
    let product_id = Uuid::new_v4();
    sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, 'HTTP product', '', 'active', 'desktop', now(), now())",
        )
        .bind(product_id)
        .bind(&product_key)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
             VALUES ($1, $2, 'active', now())",
    )
    .bind(Uuid::parse_str(&organization_id).unwrap())
    .bind(product_id)
    .execute(&pool)
    .await
    .unwrap();

    let installation_id = Uuid::new_v4();
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    let public_key = format!(
        "ed25519:v1:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(signing_key.verifying_key().as_bytes())
    );
    let (status, _, challenge) = send_method(
        &router,
        Method::POST,
        "/api/v1/enrollment-requests",
        None,
        None,
        Some(json!({
            "installation_id": installation_id,
            "product_key": product_key,
            "public_key": public_key,
            "device_label": "Caja HTTP",
            "app_version": "0.1.0",
            "platform": "windows"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let device_code = challenge["device_code"].as_str().unwrap().to_owned();
    let user_code = challenge["user_code"].as_str().unwrap().to_owned();
    let nonce = challenge["nonce"].as_str().unwrap().to_owned();
    assert_eq!(challenge["expires_in"], 600);

    let (status, _, _) = send_auth(
        &router,
        &format!("/api/v1/enrollment-requests/{user_code}/approve"),
        Some(&owner_cookie),
        Some("http://evil.example"),
        Some(json!({"organization_id": organization_id})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/installations/123e4567-e89b-42d3-a456-4266141740b2",
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "una cookie comercial no sustituye devAdminToken"
    );
    let (status, _, approved) = send_auth(
        &router,
        &format!("/api/v1/enrollment-requests/{user_code}/approve"),
        Some(&owner_cookie),
        origin,
        Some(json!({"organization_id": organization_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["status"], "approved");

    let message = canonical_proof_message(
        &device_code,
        InstallationId::from_uuid(installation_id),
        &public_key,
        &nonce,
    );
    let signature = signing_key.sign(&message);
    let proof = json!({
        "installation_id": installation_id,
        "public_key": public_key,
        "nonce": nonce,
        "signature": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())
    });
    let (status, _, completed) = send_method(
        &router,
        Method::POST,
        &format!("/api/v1/enrollment-requests/{device_code}/proof"),
        None,
        None,
        Some(proof.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(completed["installation_id"], installation_id.to_string());

    let (status, _, list) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization_id}/installations"),
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["installation_id"] == installation_id.to_string())
    );

    let (status, _, replay) = send_method(
        &router,
        Method::POST,
        &format!("/api/v1/enrollment-requests/{device_code}/proof"),
        None,
        None,
        Some(proof),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["installation_id"], installation_id.to_string());

    let (status, other_headers, _) = send_auth(
        &router,
        "/api/v1/auth/login",
        None,
        origin,
        Some(json!({"email": other_email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let other_cookie = session_from(&other_headers);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization_id}/installations"),
        Some(&other_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
