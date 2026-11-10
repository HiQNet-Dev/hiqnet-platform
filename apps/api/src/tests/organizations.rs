use super::common::*;
use super::*;

#[tokio::test]
async fn organizations_api_isolates_tenants_and_requires_session() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email_a = format!("org-a-{}@example.test", Uuid::new_v4());
    let email_b = format!("org-b-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email_a, password).await;
    seed_credential(&pool, &email_b, password).await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = Some("http://127.0.0.1:8080");

    let login = |email: String| {
        let router = router.clone();
        async move {
            let (status, headers, _) = send_auth(
                &router,
                "/api/v1/auth/login",
                None,
                Some("http://127.0.0.1:8080"),
                Some(json!({"email": email, "password": "contraseña-inicial-1"})),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            session_from(&headers)
        }
    };
    let cookie_a = login(email_a.clone()).await;
    let cookie_b = login(email_b).await;

    let (status, headers, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&cookie_a),
        origin,
        Some(json!({"display_name": "Alfa"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["role"], "owner");
    assert_eq!(body["status"], "active");
    assert!(
        headers
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("/api/v1/organizations/")
    );
    let alfa = body["id"].as_str().unwrap().to_owned();

    let (status, _, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&cookie_b),
        origin,
        Some(json!({"display_name": "Beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let beta = body["id"].as_str().unwrap().to_owned();

    let (status, _, _) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&cookie_a),
        origin,
        Some(json!({"display_name": "Alfa 2"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let unauthenticated_list = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated_list.status(), StatusCode::UNAUTHORIZED);

    // GET no usa send_auth (siempre POST); petición directa.
    let list_a = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations")
                .header(header::COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_a.status(), StatusCode::OK);
    let bytes = to_bytes(list_a.into_body(), 1024 * 1024).await.unwrap();
    let list_a: Value = serde_json::from_slice(&bytes).unwrap();
    let names: Vec<&str> = list_a["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["display_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Alfa", "Alfa 2"]);
    assert!(
        list_a["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["role"] == "owner")
    );

    let cross = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/organizations/{beta}"))
                .header(header::COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross.status(), StatusCode::NOT_FOUND);
    let own = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/organizations/{alfa}"))
                .header(header::COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);
    let invalid = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations/no-es-uuid")
                .header(header::COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    // CSRF en creación.
    let (status, _, _) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&cookie_a),
        Some("https://evil.example"),
        Some(json!({"display_name": "No permitido"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Cuenta deshabilitada: sin listado.
    let account_a: Uuid = sqlx::query_scalar(
        "SELECT account_id FROM commercial_credential c
             JOIN commercial_account a ON a.id = c.account_id
             WHERE a.email = $1",
    )
    .bind(&email_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account_a)
        .execute(&pool)
        .await
        .unwrap();
    let disabled = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/organizations")
                .header(header::COOKIE, &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn organization_creation_is_rate_limited() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("org-rate-{}@example.test", Uuid::new_v4());
    let password = "contraseña-inicial-1";
    seed_credential(&pool, &email, password).await;
    let mut app_state = state(pool.clone());
    app_state.organization_limiter = Arc::new(auth::LoginLimiter::new(
        1,
        std::time::Duration::from_secs(60),
    ));
    let router = build_router(app_state, false, test_auth_state(pool.clone()));
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
        "/api/v1/organizations",
        Some(&cookie),
        origin,
        Some(json!({"display_name": "Primera"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&cookie),
        origin,
        Some(json!({"display_name": "Segunda"})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["code"], "rate_limited");
}

#[tokio::test]
async fn members_api_enforces_roles_and_tenant_isolation() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let password = "contraseña-miembros-1";
    let owner_email = format!("members-owner-{}@example.test", Uuid::new_v4());
    let member_email = format!("members-member-{}@example.test", Uuid::new_v4());
    let foreign_email = format!("members-foreign-{}@example.test", Uuid::new_v4());
    for email in [&owner_email, &member_email, &foreign_email] {
        seed_credential(&pool, email, password).await;
    }
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = Some("http://127.0.0.1:8080");

    let login = |email: String| {
        let router = router.clone();
        async move {
            let (status, headers, _) = send_auth(
                &router,
                "/api/v1/auth/login",
                None,
                Some("http://127.0.0.1:8080"),
                Some(json!({"email": email, "password": "contraseña-miembros-1"})),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            session_from(&headers)
        }
    };
    let owner_cookie = login(owner_email.clone()).await;
    let member_cookie = login(member_email.clone()).await;
    let foreign_cookie = login(foreign_email.clone()).await;

    let account_id = |email: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT c.account_id FROM commercial_credential c
                     JOIN commercial_account a ON a.id = c.account_id
                     WHERE a.email = $1",
            )
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let owner_id = account_id(owner_email.clone()).await;
    let member_id = account_id(member_email.clone()).await;
    let owner_text = owner_id.to_string();
    let member_text = member_id.to_string();

    let (status, _, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&owner_cookie),
        origin,
        Some(json!({"display_name": "Equipo A"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let organization = body["id"].as_str().unwrap().to_owned();
    let (status, _, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&foreign_cookie),
        origin,
        Some(json!({"display_name": "Equipo B"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let foreign_organization = body["id"].as_str().unwrap().to_owned();

    let members_uri = format!("/api/v1/organizations/{organization}/members");
    let foreign_members_uri = format!("/api/v1/organizations/{foreign_organization}/members");

    // Alta por owner con Location y sin datos personales adicionales.
    let (status, headers, body) = send_auth(
        &router,
        &members_uri,
        Some(&owner_cookie),
        origin,
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(
        headers
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with(&member_text)
    );

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &members_uri,
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let owner_item = items
        .iter()
        .find(|item| item["account_id"].as_str() == Some(owner_text.as_str()))
        .unwrap();
    assert_eq!(owner_item["role"], "owner");
    let member_item = items
        .iter()
        .find(|item| item["account_id"].as_str() == Some(member_text.as_str()))
        .unwrap();
    assert_eq!(member_item["role"], "member");
    assert!(member_item["joined_at"].is_string());
    for item in items {
        assert!(item.get("email").is_none());
        assert!(item.get("display_name").is_none());
        assert!(item.get("password_hash").is_none());
    }

    // Cualquier miembro activo consulta.
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &members_uri,
        Some(&member_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // admin/member no administran.
    let (status, _, _) = send_auth(
        &router,
        &members_uri,
        Some(&member_cookie),
        origin,
        Some(json!({"account_id": owner_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_method(
        &router,
        Method::PATCH,
        &format!("{members_uri}/{owner_text}"),
        Some(&member_cookie),
        origin,
        Some(json!({"role": "owner"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_method(
        &router,
        Method::DELETE,
        &format!("{members_uri}/{owner_text}"),
        Some(&member_cookie),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Tenant ajeno: 404 neutral en consulta y mutación.
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &foreign_members_uri,
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send_auth(
        &router,
        &foreign_members_uri,
        Some(&owner_cookie),
        origin,
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Rol inválido antes de tocar la base.
    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("{members_uri}/{member_text}"),
        Some(&owner_cookie),
        origin,
        Some(json!({"role": "superuser"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "validation_error");

    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("{members_uri}/{member_text}"),
        Some(&owner_cookie),
        origin,
        Some(json!({"role": "admin"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "admin");
    let (status, _, _) = send_method(
        &router,
        Method::PATCH,
        &format!("{members_uri}/{member_text}"),
        Some(&owner_cookie),
        origin,
        Some(json!({"role": "admin"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Duplicado y reincorporación tras revocación.
    let (status, _, _) = send_auth(
        &router,
        &members_uri,
        Some(&owner_cookie),
        origin,
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, _) = send_method(
        &router,
        Method::DELETE,
        &format!("{members_uri}/{member_text}"),
        Some(&owner_cookie),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &members_uri,
        Some(&member_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "la revocación quita acceso");
    let (status, _, _) = send_auth(
        &router,
        &members_uri,
        Some(&owner_cookie),
        origin,
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, _) = send_method(
        &router,
        Method::DELETE,
        &format!("{members_uri}/{owner_text}"),
        Some(&owner_cookie),
        origin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "último owner protegido");

    // CSRF y sesión ausente.
    let (status, _, _) = send_auth(
        &router,
        &members_uri,
        Some(&owner_cookie),
        Some("https://evil.example"),
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_method(&router, Method::GET, &members_uri, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Paginación de organizaciones compatible con P02.3-A.
    for name in ["Equipo C", "Equipo D", "Equipo E"] {
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/organizations",
            Some(&owner_cookie),
            origin,
            Some(json!({"display_name": name})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let mut cursor: Option<String> = None;
    let mut organization_cursor: Option<String> = None;
    let mut names = Vec::new();
    loop {
        let uri = match &cursor {
            Some(cursor) => format!("/api/v1/organizations?limit=1&cursor={cursor}"),
            None => "/api/v1/organizations?limit=1".to_owned(),
        };
        let (status, _, body) =
            send_method(&router, Method::GET, &uri, Some(&owner_cookie), None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["items"].as_array().unwrap().len() <= 1);
        names.extend(
            body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["display_name"].as_str().unwrap().to_owned()),
        );
        cursor = body["next_cursor"].as_str().map(str::to_owned);
        if organization_cursor.is_none() {
            organization_cursor = cursor.clone();
        }
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(names, ["Equipo A", "Equipo C", "Equipo D", "Equipo E"]);
    let organization_cursor = organization_cursor.expect("cursor de organizaciones");

    // Cursores ligados al tipo de listado y, para miembros, a la organización.
    let (status, _, body) = send_auth(
        &router,
        "/api/v1/organizations",
        Some(&owner_cookie),
        origin,
        Some(json!({"display_name": "Equipo F"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let team_f = body["id"].as_str().unwrap().to_owned();
    let (status, _, _) = send_auth(
        &router,
        &format!("/api/v1/organizations/{team_f}/members"),
        Some(&owner_cookie),
        origin,
        Some(json!({"account_id": member_id, "role": "member"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{team_f}/members?limit=1"),
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let member_cursor = body["next_cursor"].as_str().unwrap().to_owned();

    for uri in [
        format!("/api/v1/organizations/{organization}/members?cursor={member_cursor}"),
        format!("/api/v1/organizations/{organization}/members?cursor={organization_cursor}"),
        format!("/api/v1/organizations?cursor={member_cursor}"),
    ] {
        let (status, _, _) =
            send_method(&router, Method::GET, &uri, Some(&owner_cookie), None, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
    let oversized = "a".repeat(2049);
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/organizations/{organization}/members?cursor={oversized}"),
        Some(&owner_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    for uri in [
        "/api/v1/organizations?limit=0",
        "/api/v1/organizations?limit=101",
        "/api/v1/organizations?cursor=no-es-un-cursor",
    ] {
        let (status, _, _) =
            send_method(&router, Method::GET, uri, Some(&owner_cookie), None, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
}
