use super::*;

pub(crate) fn isolated_test_url() -> String {
    let url = std::env::var("TEST_DATABASE_URL").expect("base aislada requerida");
    assert!(
        url.rsplit('/')
            .next()
            .is_some_and(|database| database.contains("test") && database != "hiqnet"),
        "TEST_DATABASE_URL debe identificar una base aislada llamada test"
    );
    url
}

pub(crate) fn state(pool: PgPool) -> AppState {
    AppState {
        service: Arc::new(PlatformService::new(
            Arc::new(pool_repository(pool.clone())),
            None,
        )),
        commercial: Arc::new(CommercialService::new(PgCommercialRepository::new(
            pool.clone(),
        ))),
        commercial_domain: Arc::new(
            platform_application::commercial_domain::CommercialService::new(
                platform_storage_postgres::PgCommercialDomainRepository::new(pool.clone()),
            ),
        ),
        enrollment: Arc::new(EnrollmentService::new(
            PgEnrollmentRepository::new(pool.clone()),
            platform_core::enrollment::DEFAULT_CHALLENGE_TTL_SECONDS,
            "http://127.0.0.1:8080/device".into(),
        )),
        organization_limiter: Arc::new(auth::LoginLimiter::new(
            5,
            std::time::Duration::from_secs(60),
        )),
        enrollment_begin_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        enrollment_proof_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        enrollment_approve_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        admin_token: None,
        licensing_enabled: false,
        db_pool: pool,
    }
}

pub(crate) fn test_auth_state(pool: PgPool) -> AuthState {
    AuthState {
        service: Arc::new(platform_application::AuthService::new(
            platform_storage_postgres::PgAuthRepository::new(pool.clone()),
        )),
        cookie_secure: false,
        allowed_origins: vec!["http://127.0.0.1:8080".into()],
        development: true,
        session_ttl: time::Duration::hours(1),
        limiter: Arc::new(auth::LoginLimiter::new(
            10,
            std::time::Duration::from_secs(60),
        )),
        argon2_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        staff: staff::StaffState {
            service: Arc::new(platform_application::staff::StaffAuthService::new(
                platform_storage_postgres::PgStaffRepository::new(pool.clone()),
            )),
            product: Arc::new(platform_application::product::ProductService::new(
                platform_storage_postgres::PgProductRepository::new(pool.clone()),
            )),
            control: Arc::new(platform_application::control::ControlService::new(
                platform_storage_postgres::PgControlRepository::new(pool.clone()),
            )),
            audit_security: Arc::new(
                platform_application::audit_security::AuditSecurityService::new(
                    platform_storage_postgres::PgAuditSecurityRepository::new(pool.clone()),
                ),
            ),
            organization_products: Arc::new(
                platform_application::organization_product::OrganizationProductService::new(
                    platform_storage_postgres::PgOrganizationProductRepository::new(pool.clone()),
                ),
            ),
            commercial: Arc::new(
                platform_application::commercial_domain::CommercialService::new(
                    platform_storage_postgres::PgCommercialDomainRepository::new(pool.clone()),
                ),
            ),
            installations: Arc::new(
                platform_application::installation_control::InstallationControlService::new(
                    platform_storage_postgres::PgInstallationControlRepository::new(pool),
                ),
            ),
            limiter: Arc::new(auth::LoginLimiter::new(
                10,
                std::time::Duration::from_secs(60),
            )),
            session_ttl: time::Duration::hours(1),
            argon2_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        },
    }
}

pub(crate) fn dev_state(pool: PgPool, licensing_enabled: bool) -> AppState {
    // Semilla efímera para pruebas; nunca se almacena ni se publica.
    let mut seed = [0_u8; 32];
    seed[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    seed[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let signer: SharedSigner = Arc::new(
        platform_licensing::Ed25519Signer::from_seed_base64(
            &base64::engine::general_purpose::STANDARD.encode(seed),
        )
        .unwrap(),
    );
    AppState {
        service: Arc::new(PlatformService::new(
            Arc::new(pool_repository(pool.clone())),
            Some(signer),
        )),
        commercial: Arc::new(CommercialService::new(PgCommercialRepository::new(
            pool.clone(),
        ))),
        commercial_domain: Arc::new(
            platform_application::commercial_domain::CommercialService::new(
                platform_storage_postgres::PgCommercialDomainRepository::new(pool.clone()),
            ),
        ),
        enrollment: Arc::new(EnrollmentService::new(
            PgEnrollmentRepository::new(pool.clone()),
            platform_core::enrollment::DEFAULT_CHALLENGE_TTL_SECONDS,
            "http://127.0.0.1:8080/device".into(),
        )),
        organization_limiter: Arc::new(auth::LoginLimiter::new(
            5,
            std::time::Duration::from_secs(60),
        )),
        enrollment_begin_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        enrollment_proof_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        enrollment_approve_limiter: Arc::new(auth::LoginLimiter::new(
            1000,
            std::time::Duration::from_secs(60),
        )),
        admin_token: Some("token-sintetico-p01".into()),
        licensing_enabled,
        db_pool: pool,
    }
}

pub(crate) async fn test_organization(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Organización sintética', 'active', now())",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

pub(crate) async fn remove_test_organization(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM organization WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

pub(crate) async fn product_project_audit_count(pool: &PgPool, email: &str) -> i64 {
    sqlx::query_scalar(
            "SELECT count(*) FROM staff_audit_event e
             JOIN staff_account a ON a.id = e.staff_account_id
             WHERE a.email = $1
               AND e.action IN ('product_created', 'product_updated', 'project_created', 'project_updated')",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub(crate) async fn send(
    router: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let payload = if let Some(body) = body {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(request.body(payload).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

pub(crate) async fn seed_credential(pool: &PgPool, email: &str, password: &str) {
    use platform_application::auth::AuthRepository as _;
    let now = time::OffsetDateTime::now_utc();
    let account = platform_application::commercial::CommercialService::new(
        platform_storage_postgres::PgCommercialRepository::new(pool.clone()),
    )
    .create_account(email, "Cuenta API", now)
    .await
    .unwrap();
    let hash = platform_application::security::hash_password(password.to_owned())
        .await
        .unwrap();
    platform_storage_postgres::PgAuthRepository::new(pool.clone())
        .create_credential(&platform_core::auth::CommercialCredential::new(
            account.id, hash, now,
        ))
        .await
        .unwrap();
}

pub(crate) async fn send_auth(
    router: &Router,
    uri: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    send_method(router, Method::POST, uri, cookie, origin, body).await
}

pub(crate) async fn send_method(
    router: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    send_method_from(
        router,
        method,
        uri,
        cookie,
        origin,
        body,
        std::net::SocketAddr::from(([127, 0, 0, 1], 40000)),
    )
    .await
}

pub(crate) async fn send_method_from(
    router: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: Option<Value>,
    peer: std::net::SocketAddr,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    let payload = if let Some(body) = body {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let mut request = request.body(payload).unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(peer));
    request
        .extensions_mut()
        .insert(crate::enrollment::PeerIp::for_test(peer.ip()));
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub(crate) fn session_from(headers: &HeaderMap) -> String {
    let raw = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(raw.contains("HttpOnly"));
    assert!(raw.contains("SameSite=Lax"));
    assert!(raw.contains("Path=/api"));
    raw.split(';').next().unwrap().to_owned()
}

pub(crate) async fn seed_staff(
    pool: &PgPool,
    email: &str,
    password: &str,
    role: platform_core::staff::StaffRole,
) {
    let now = time::OffsetDateTime::now_utc();
    let hash = platform_application::security::hash_password(password.to_owned())
        .await
        .unwrap();
    platform_storage_postgres::PgStaffRepository::new(pool.clone())
        .create_staff(email, "Staff API", role, &hash, now)
        .await
        .unwrap();
}

pub(crate) fn staff_session_from(headers: &HeaderMap) -> String {
    let raw = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(raw.contains("HttpOnly"));
    assert!(raw.contains("SameSite=Strict"));
    assert!(raw.contains("Path=/api/v1/control"));
    assert!(raw.starts_with("hiqnet_staff_session="));
    raw.split(';').next().unwrap().to_owned()
}

pub(crate) fn assert_rfc3339(value: &Value, field: &str) {
    let text = value
        .as_str()
        .unwrap_or_else(|| panic!("{field} debe ser string RFC3339: {value}"));
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|error| panic!("{field} no parsea como RFC3339: {text}: {error}"));
}

pub(crate) fn random_bytes32() -> Vec<u8> {
    let mut bytes = Uuid::new_v4().as_bytes().to_vec();
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes
}

pub(crate) async fn control_login(router: &Router, email: &str, password: &str) -> String {
    let (status, headers, _) = send_auth(
        router,
        "/api/v1/control/auth/login",
        None,
        Some("http://127.0.0.1:8080"),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    staff_session_from(&headers)
}
