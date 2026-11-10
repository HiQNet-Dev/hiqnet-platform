use super::common::*;
use super::*;

#[tokio::test]
async fn down_database_returns_normalized_error_without_sql() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
        .unwrap();
    let router = build_router(
        dev_state(pool.clone(), false),
        false,
        test_auth_state(pool.clone()),
    );
    let path = format!("/api/v1/installations/{}", Uuid::new_v4());
    let (status, error) = send(
        &router,
        Method::GET,
        &path,
        Some("token-sintetico-p01"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["code"], "storage_error");
    assert!(!error.to_string().contains("SELECT"));
    assert!(!error.to_string().contains("postgres://"));
}

#[test]
fn enrollment_invalid_maps_to_conflict_not_forbidden() {
    let (status, body) = crate::enrollment::enrollment_error(EnrollmentError::EnrollmentInvalid);
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body.0.code, "enrollment_invalid");
}

#[test]
fn storage_errors_are_not_client_errors() {
    assert_eq!(err(AppError::Storage).0, StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn no_admin_token_rejects_even_empty_bearer_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        "Bearer ".parse().unwrap(),
    );
    assert!(matches!(
        admin_guard(&headers, &None),
        Err(AppError::AdminDisabled)
    ));
}
