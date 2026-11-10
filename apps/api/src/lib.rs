//! API HTTP de HiQNet Platform.
//!
//! Seguridad: las operaciones de instalaciones y licencias exigen un token
//! administrativo DEV (`HIQNET_PLATFORM_ADMIN_TOKEN`). Sin él, se rechazan con 401.
//!
//! El crate es un **monolito modular**: el dominio y la aplicación viven en sus
//! propios crates; este crate es el adaptador de entrada HTTP y el composition
//! root. `lib.rs` sólo declara módulos y arranca el proceso.

use auth::{AuthContext, AuthState};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use contract::{
    ErrorResponse, HealthResponse, InstallationResponse, IssueLicenseBody, IssuedLicenseResponse,
    LicenseRecordResponse, PlanResponse, ReadinessResponse, RegisterInstallationBody,
};
use platform_application::{
    AppError, PlatformService, SharedRepository, SharedSigner,
    commercial::{CommercialService, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, OrganizationView},
    enrollment::EnrollmentService,
};
use platform_core::DomainError;
use platform_core::commercial::{CommercialError, OrganizationMembership, OrganizationRole};
use platform_core::ids::{CommercialAccountId, InstallationId, OrganizationId};
use platform_storage_postgres::{
    PgCommercialRepository, PgEnrollmentRepository, is_ready, open, pool_repository,
};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::openapi::{
    Info, OpenApi as OpenApiDocument,
    security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

mod app_state;
mod auth;
mod commercial_auth;
mod composition;
mod config;
mod contract;
mod control;
mod control_audit;
mod control_commercial;
mod control_directory;
mod control_installations;
mod control_organization_products;
mod enrollment;
mod health;
mod licensing;
mod organizations;
mod portal_commercial;
mod router;
mod staff;

#[cfg(test)]
mod tests;

pub use app_state::AppState;
pub use router::{build_router, openapi_document};
// Utilidades HTTP compartidas: se reexportan para que el glob `use crate::*`
// de cada módulo las encuentre igual que cuando vivían en `lib.rs`.
pub(crate) use app_state::{
    HttpError, admin_guard, err, json_error, path_error, path_id, query_error,
};

use config::Settings;

/// Arranca el servidor HTTP: configuración, composición y `serve`.
pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();

    let settings = Settings::from_env()?;
    let pool = open(&settings.database_url).await?;
    let (state, auth_state) = composition::assemble(&settings, pool)?;
    let app = build_router(state, settings.docs_enabled, auth_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", settings.port)).await?;
    tracing::info!(port = settings.port, "HiQNet Platform API listening");
    // ConnectInfo permite limitar begin/proof por peer real. No se confían
    // X-Forwarded-For ni Forwarded sin una lista explícita de proxies.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
