//! Pruebas de control en el Control Center.

use super::common::*;
use super::*;

#[test]
fn control_openapi_exposes_product_enums() {
    let document = serde_json::to_value(openapi_document()).unwrap();
    let status = &document["components"]["schemas"]["ProductStatusDto"]["enum"];
    let statuses: Vec<&str> = status
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"active"));
    assert!(statuses.contains(&"retired"));
    let product_type = &document["components"]["schemas"]["ProductTypeDto"]["enum"];
    assert!(
        product_type
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "desktop")
    );
    let project_type = &document["components"]["schemas"]["ProjectTypeDto"]["enum"];
    assert!(
        project_type
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "infrastructure")
    );
}

#[tokio::test]
async fn control_products_and_projects_enforce_rbac_and_audit() {
    let pool = open(&isolated_test_url()).await.unwrap();
    let email = format!("staff-admin-{}@hiqnet.test", Uuid::new_v4());
    let password = "contraseña-staff-larga";
    seed_staff(
        &pool,
        &email,
        password,
        platform_core::staff::StaffRole::SuperAdmin,
    )
    .await;
    let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
    let origin = "http://127.0.0.1:8080";

    // Sin sesión staff no se accede al Control Center.
    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/products",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some(origin),
        Some(json!({"email": email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let admin_cookie = staff_session_from(&headers);

    let product_key = format!("pos-{}", Uuid::new_v4().simple());
    let (status, _, body) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/products",
        Some(&admin_cookie),
        Some(origin),
        Some(json!({
            "key": product_key,
            "name": "HiQNet POS",
            "description": "Punto de venta",
            "status": "active",
            "type": "desktop",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["key"], product_key);
    assert_eq!(body["status"], "active");
    assert_rfc3339(&body["created_at"], "product.created_at");
    assert_rfc3339(&body["updated_at"], "product.updated_at");
    let product_initial_updated_at = body["updated_at"].clone();

    // Clave duplicada: conflicto sin filtrar detalles.
    let (status, _, _) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/products",
        Some(&admin_cookie),
        Some(origin),
        Some(json!({
            "key": product_key,
            "name": "Duplicado",
            "status": "active",
            "type": "desktop",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "HiQNet POS");

    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        Some(origin),
        Some(json!({"status": "maintenance"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "maintenance");
    assert_rfc3339(&body["updated_at"], "product.updated_at tras PATCH");
    let product_changed_updated_at = body["updated_at"].clone();
    assert_ne!(product_changed_updated_at, product_initial_updated_at);
    assert_eq!(product_project_audit_count(&pool, &email).await, 2);

    // PATCH sin cambios efectivos → 400 (no se audita una mutación inexistente).
    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        Some(origin),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "validation_error");

    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        Some(origin),
        Some(json!({"status": null})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "validation_error");

    // Repetir el valor persistido también es un no-op: no se actualiza ni audita.
    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        Some(origin),
        Some(json!({"status": "maintenance"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "validation_error");

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/products/{product_key}"),
        Some(&admin_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["updated_at"], product_changed_updated_at);
    assert_eq!(product_project_audit_count(&pool, &email).await, 2);

    let project_key = format!("api-{}", Uuid::new_v4().simple());
    let (status, _, body) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/projects",
        Some(&admin_cookie),
        Some(origin),
        Some(json!({
            "key": project_key,
            "name": "HiQNet Platform API",
            "project_type": "service",
            "status": "active",
            "product_key": product_key,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["product_key"], product_key);
    assert_rfc3339(&body["created_at"], "project.created_at");
    assert_rfc3339(&body["updated_at"], "project.updated_at");
    let project_initial_updated_at = body["updated_at"].clone();

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/projects?product_key={product_key}"),
        Some(&admin_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["key"] == project_key)
    );

    let (status, _, body) = send_method(
        &router,
        Method::PATCH,
        &format!("/api/v1/control/projects/{project_key}"),
        Some(&admin_cookie),
        Some(origin),
        Some(json!({"status": "maintenance"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "maintenance");
    let project_changed_updated_at = body["updated_at"].clone();
    assert_ne!(project_changed_updated_at, project_initial_updated_at);
    assert_eq!(product_project_audit_count(&pool, &email).await, 4);

    for request in [
        json!({}),
        json!({"status": null}),
        json!({"status": "maintenance"}),
    ] {
        let (status, _, body) = send_method(
            &router,
            Method::PATCH,
            &format!("/api/v1/control/projects/{project_key}"),
            Some(&admin_cookie),
            Some(origin),
            Some(request),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "validation_error");
    }

    let (status, _, body) = send_method(
        &router,
        Method::GET,
        &format!("/api/v1/control/projects/{project_key}"),
        Some(&admin_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["updated_at"], project_changed_updated_at);
    assert_eq!(product_project_audit_count(&pool, &email).await, 4);

    // read_only puede leer pero no mutar.
    let reader_email = format!("staff-reader-{}@hiqnet.test", Uuid::new_v4());
    seed_staff(
        &pool,
        &reader_email,
        password,
        platform_core::staff::StaffRole::ReadOnly,
    )
    .await;
    let (status, headers, _) = send_auth(
        &router,
        "/api/v1/control/auth/login",
        None,
        Some(origin),
        Some(json!({"email": reader_email, "password": password})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let reader_cookie = staff_session_from(&headers);

    let (status, _, _) = send_method(
        &router,
        Method::GET,
        "/api/v1/control/products",
        Some(&reader_cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, body) = send_method(
        &router,
        Method::POST,
        "/api/v1/control/products",
        Some(&reader_cookie),
        Some(origin),
        Some(json!({
            "key": format!("forbidden-{}", Uuid::new_v4().simple()),
            "name": "No permitido",
            "status": "active",
            "type": "internal",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "forbidden");
}
