//! API HTTP de HiQNet Platform.
//!
//! Seguridad: las operaciones de escritura exigen un token administrativo
//! (`HIQNET_PLATFORM_ADMIN_TOKEN`). Sin token configurado, se rechazan con 401.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use platform_application::{
    AppError, InstallationView, IssueLicenseRequest, IssuedLicenseView, PlatformService,
    RegisterInstallationRequest, SharedRepository, SharedSigner,
};
use platform_core::ids::InstallationId;
use platform_storage_postgres::{open, pool_repository};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<PlatformService>,
    pub admin_token: Option<String>,
    pub licensing_enabled: bool,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    database: &'static str,
}

#[derive(Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
}

fn err(error: AppError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        AppError::NotFound => StatusCode::NOT_FOUND,
        AppError::Unauthorized | AppError::AdminDisabled => StatusCode::UNAUTHORIZED,
        AppError::LicensingDisabled => StatusCode::SERVICE_UNAVAILABLE,
        AppError::Conflict => StatusCode::CONFLICT,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(ErrorResponse {
            code: error.code(),
            message: error.to_string(),
        }),
    )
}

fn admin_guard(headers: &HeaderMap, token: &Option<String>) -> Result<(), AppError> {
    let Some(expected) = token else {
        return Err(AppError::AdminDisabled);
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/plans", get(list_plans))
        .route("/api/v1/installations", post(register_installation))
        .route("/api/v1/installations/{id}", get(get_installation))
        .route("/api/v1/installations/{id}/trial", post(start_trial))
        .route("/api/v1/installations/{id}/licenses", post(issue_license))
        .route("/api/v1/installations/{id}/licenses", get(list_licenses))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let _ = &state.service;
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database: "up",
    })
}

async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<Vec<platform_application::PlanView>>, (StatusCode, Json<ErrorResponse>)> {
    state.service.list_plans().await.map(Json).map_err(err)
}

async fn register_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterInstallationRequest>,
) -> Result<(StatusCode, Json<InstallationView>), (StatusCode, Json<ErrorResponse>)> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .register_installation(request, time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn get_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<InstallationView>, (StatusCode, Json<ErrorResponse>)> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .get_installation(InstallationId::from_uuid(id))
        .await
        .map_err(err)?;
    Ok(Json(view))
}

async fn start_trial(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<InstallationView>, (StatusCode, Json<ErrorResponse>)> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .start_trial(
            InstallationId::from_uuid(id),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(err)?;
    Ok(Json(view))
}

async fn issue_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<IssueLicenseRequest>,
) -> Result<(StatusCode, Json<IssuedLicenseView>), (StatusCode, Json<ErrorResponse>)> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    if !state.licensing_enabled {
        return Err(err(AppError::LicensingDisabled));
    }
    let view = state
        .service
        .issue_license(
            InstallationId::from_uuid(id),
            request,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn list_licenses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<platform_core::license::IssuedLicense>>, (StatusCode, Json<ErrorResponse>)> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let licenses = state
        .service
        .list_licenses(InstallationId::from_uuid(id))
        .await
        .map_err(err)?;
    Ok(Json(licenses))
}

pub fn documented_paths() -> Vec<&'static str> {
    vec![
        "/health",
        "/api/v1/plans",
        "/api/v1/installations",
        "/api/v1/installations/{id}",
        "/api/v1/installations/{id}/trial",
        "/api/v1/installations/{id}/licenses",
    ]
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hiqnet:hiqnet_dev@localhost:5432/hiqnet".to_owned());
    let pool = open(&database_url).await?;
    let repository: SharedRepository = Arc::new(pool_repository(pool));

    let signing_key = std::env::var("HIQNET_PLATFORM_SIGNING_KEY_B64").ok();
    let signer = signing_key
        .as_deref()
        .map(platform_licensing::Ed25519Signer::from_seed_base64)
        .transpose()
        .map_err(|error| anyhow::anyhow!(error))?
        .map(|signer| Arc::new(signer) as SharedSigner);

    let licensing_enabled = signer.is_some()
        && std::env::var("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE")
            .map(|value| value == "true")
            .unwrap_or(false);

    let admin_token = std::env::var("HIQNET_PLATFORM_ADMIN_TOKEN").ok();

    let service = Arc::new(PlatformService::new(repository, signer));
    let state = AppState {
        service,
        admin_token,
        licensing_enabled,
    };

    let app = build_router(state);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "HiQNet Platform API listening");
    axum::serve(listener, app).await?;
    Ok(())
}
