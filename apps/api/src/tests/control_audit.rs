//! Pruebas de control_audit en el Control Center.

use super::common::*;
use super::*;

#[tokio::test]
async fn control_audit_security_and_staff_auth_are_read_only_and_sanitized() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("audit-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::SecurityAuditor,
    )
    .await;
    let staff_id: Uuid = sqlx::query_scalar("SELECT id FROM staff_account WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();

    let now = time::OffsetDateTime::now_utc();
    let audit_ids = [Uuid::new_v4(), Uuid::new_v4()];
    for (index, id) in audit_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO staff_audit_event
                 (id, staff_account_id, action, resource_type, result, occurred_at, metadata)
                 VALUES ($1, $2, 'installation_suspended', 'installation', 'success', $3, $4)",
        )
        .bind(id)
        .bind(staff_id)
        .bind(now - time::Duration::seconds(index as i64))
        .bind(json!({
            "before": "active",
            "after": "suspended",
            "token_hash": "never",
            "password_hash": "never"
        }))
        .execute(&pool)
        .await
        .unwrap();
    }
    let security_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO commercial_security_event (id, kind, metadata, created_at)
             VALUES ($1, 'rehost_performed', $2, $3)",
    )
    .bind(security_id)
    .bind(json!({
        "enrollment_request_id": "request-id",
        "nonce": "never"
    }))
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let auth_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO staff_auth_event (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, 'login_failure', $3, $4)",
    )
    .bind(auth_id)
    .bind(staff_id)
    .bind(now)
    .bind(json!({"reason": "bad_password", "ip_hash": "never"}))
    .execute(&pool)
    .await
    .unwrap();

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

    // Sin sesión staff: 401 neutral.
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/audit",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, audit_page) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/audit?limit=1",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(audit_page["items"].as_array().unwrap().len(), 1);
    let cursor = audit_page["next_cursor"].as_str().unwrap().to_owned();
    assert!(!audit_page.to_string().contains("token_hash"));

    // Cursor de Audit no se reutiliza en Security.
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/security-events?cursor={cursor}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, audit_detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/audit/{}", audit_ids[0]),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(audit_detail["actor"]["email"], json!(email));
    assert_eq!(audit_detail["metadata"]["before"], json!("active"));
    assert_eq!(audit_detail["metadata"]["after"], json!("suspended"));
    assert!(audit_detail["metadata"]["token_hash"].is_null());

    let (status, _, security_detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/security-events/{security_id}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        security_detail["metadata"],
        json!({"enrollment_request_id": "request-id"})
    );
    assert!(security_detail["metadata"]["nonce"].is_null());

    let (status, _, auth_detail) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/staff-auth-events/{auth_id}"),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(auth_detail["metadata"], json!({"reason": "bad_password"}));
    assert!(auth_detail["metadata"]["ip_hash"].is_null());

    // Validación y 404 neutral.
    for path in [
        "/api/v1/control/audit?limit=0",
        "/api/v1/control/audit?cursor=not-base64",
        "/api/v1/control/audit/not-a-uuid",
        "/api/v1/control/audit?from=2026-13-01T00:00:00Z",
    ] {
        let (status, _, _) =
            send_method(&router, Method::GET, path, Some(&cookie), None, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/audit/{}", Uuid::new_v4()),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], json!("not_found"));
}
