//! Pruebas de control_organization_products en el Control Center.

use super::common::*;
use super::*;

#[tokio::test]
async fn organization_product_mutations_are_idempotent_audited_and_rbac_checked() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let admin_email = format!("op-admin-{}@hiqnet.test", Uuid::new_v4());
    let reader_email = format!("op-reader-{}@hiqnet.test", Uuid::new_v4());
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
             VALUES ($1, 'OP API org', 'active', now())",
    )
    .bind(organization)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, 'OP API product', '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(format!("op-api-{}", Uuid::new_v4().simple()))
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

    let collection = format!("/api/v1/control/organizations/{organization}/products");
    let item = format!("{collection}/{product}");
    let body = Some(json!({"product_id": product}));

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &collection,
        Some(&reader),
        origin,
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, first) = send_method(
        &router,
        Method::POST,
        &collection,
        Some(&admin),
        origin,
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["changed"], json!(true));
    assert_eq!(first["product"]["association_status"], json!("active"));

    let (status, _, second) = send_method(
        &router,
        Method::POST,
        &collection,
        Some(&admin),
        origin,
        body,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["changed"], json!(false));

    let (status, _, list) =
        send_method(&router, Method::GET, &collection, Some(&reader), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);

    let (status, _, inactive) = send_method(
        &router,
        Method::PATCH,
        &item,
        Some(&admin),
        origin,
        Some(json!({"status": "inactive"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inactive["changed"], json!(true));
    assert_eq!(inactive["product"]["association_status"], json!("inactive"));

    let (status, _, _) = send_method(
        &router,
        Method::PATCH,
        &item,
        Some(&admin),
        origin,
        Some(json!({"status": "deleted"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, _) = send_method(
        &router,
        Method::POST,
        &collection,
        Some(&admin),
        origin,
        Some(json!({"product_id": Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/organizations/{}/products", Uuid::new_v4()),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_audit_event
             WHERE organization_id = $1 AND product_id = $2
               AND action IN ('organization_product_activated', 'organization_product_deactivated')",
        )
        .bind(organization)
        .bind(product)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audits, 2);
}
