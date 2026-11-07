//! API HTTP de HiQNet Platform.
//!
//! Seguridad: las operaciones de instalaciones y licencias exigen un token
//! administrativo DEV (`HIQNET_PLATFORM_ADMIN_TOKEN`). Sin él, se rechazan con 401.

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
mod auth;
mod contract;
use auth::{AuthContext, AuthState};
use contract::{
    ErrorResponse, HealthResponse, InstallationResponse, IssueLicenseBody, IssuedLicenseResponse,
    LicenseRecordResponse, PlanResponse, ReadinessResponse, RegisterInstallationBody,
};
use platform_application::{AppError, PlatformService, SharedRepository, SharedSigner};
use platform_core::DomainError;
use platform_core::ids::InstallationId;
use platform_storage_postgres::{is_ready, open, pool_repository};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::openapi::{
    Info, OpenApi as OpenApiDocument,
    security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<PlatformService>,
    pub admin_token: Option<String>,
    pub licensing_enabled: bool,
    pub db_pool: PgPool,
}

fn err(error: AppError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        AppError::NotFound | AppError::Domain(DomainError::InstallationNotFound) => {
            StatusCode::NOT_FOUND
        }
        AppError::Unauthorized | AppError::AdminDisabled => StatusCode::UNAUTHORIZED,
        AppError::LicensingDisabled
        | AppError::Storage
        | AppError::Domain(DomainError::LicensingDisabled) => StatusCode::SERVICE_UNAVAILABLE,
        AppError::Conflict
        | AppError::Domain(
            DomainError::DuplicateInstallation | DomainError::TrialAlreadyActivated,
        ) => StatusCode::CONFLICT,
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

type HttpError = (StatusCode, Json<ErrorResponse>);

fn json_error(rejection: JsonRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_request",
            message: "cuerpo JSON inválido o incompatible con el contrato".into(),
        }),
    )
}

fn path_error(rejection: PathRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_path",
            message: "identificador de ruta inválido".into(),
        }),
    )
}

fn path_id(path: Result<Path<Uuid>, PathRejection>) -> Result<InstallationId, HttpError> {
    path.map(|Path(id)| InstallationId::from_uuid(id))
        .map_err(path_error)
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

fn api_routes() -> (Router<AppState>, OpenApiDocument) {
    let (router, mut document) = OpenApiRouter::<AppState>::new()
        .routes(routes!(health))
        .routes(routes!(ready))
        .routes(routes!(list_plans))
        .routes(routes!(register_installation))
        .routes(routes!(get_installation))
        .routes(routes!(start_trial))
        .routes(routes!(issue_license, list_licenses))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .split_for_parts();
    document.info = Info::new("HiQNet Platform API", env!("CARGO_PKG_VERSION"));
    document.info.description = Some("Plano de control técnico; sin identidad comercial, facturación ni sincronización productivas. El token administrativo es temporal y exclusivo de desarrollo.".into());
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
        "devAdminToken",
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .description(Some(
                    "Token administrativo temporal para pruebas DEV; no es una sesión de usuario.",
                ))
                .build(),
        ),
    );
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "commercialSession",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "hiqnet_session",
                "Cookie de sesión comercial HttpOnly emitida por /api/v1/auth/login.",
            ))),
        );
    (router, document)
}

pub fn openapi_document() -> OpenApiDocument {
    api_routes().1
}

pub fn build_router(state: AppState, docs_enabled: bool, auth_state: AuthState) -> Router {
    let (router, document) = api_routes();
    let router = router
        .with_state(state)
        .layer(axum::Extension(auth_state.clone()));
    if docs_enabled {
        router.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", document))
    } else {
        router
    }
}

/// Comprueba únicamente que el proceso HTTP responde; no implica disponibilidad de PostgreSQL.
#[utoipa::path(get, path = "/health", operation_id = "getHealth", tag = "Operación",
    responses((status = 200, description = "Proceso HTTP activo; no comprueba PostgreSQL", body = HealthResponse,
        example = json!({"status":"ok","version":"0.1.0"}))))]
async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Ejecuta un roundtrip real a PostgreSQL para determinar si la API puede atender tráfico.
#[utoipa::path(get, path = "/ready", operation_id = "getReadiness", tag = "Operación",
    responses(
        (status = 200, description = "PostgreSQL respondió a SELECT 1", body = ReadinessResponse,
            example = json!({"status":"ok","database":"up"})),
        (status = 503, description = "PostgreSQL no disponible", body = ReadinessResponse,
            example = json!({"status":"unavailable","database":"down"}))))]
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    if is_ready(&state.db_pool).await {
        (
            StatusCode::OK,
            Json(ReadinessResponse {
                status: "ok",
                database: "up",
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                status: "unavailable",
                database: "down",
            }),
        )
    }
}

/// Devuelve el catálogo técnico. Los nombres de plan no implican disponibilidad comercial.
#[utoipa::path(get, path = "/api/v1/plans", operation_id = "listPlans", tag = "Planes",
    responses(
        (status = 200, description = "Catálogo de planes", body = [PlanResponse],
            example = json!([{"plan":"trial","display_name":"Trial (30 días)","features":["catalog","sales"]}]))))]
async fn list_plans(State(state): State<AppState>) -> Result<Json<Vec<PlanResponse>>, HttpError> {
    state
        .service
        .list_plans()
        .await
        .map(|plans| Json(plans.into_iter().map(Into::into).collect()))
        .map_err(err)
}

/// Registra una instalación técnica; requiere el token administrativo DEV.
#[utoipa::path(post, path = "/api/v1/installations", operation_id = "registerInstallation", tag = "Instalaciones",
    request_body(content = RegisterInstallationBody, description = "organization_id UUID y device_label no vacío; installation_id UUID opcional"),
    security(("devAdminToken" = [])),
    responses(
        (status = 201, description = "Instalación registrada", body = InstallationResponse),
        (status = 400, description = "Etiqueta vacía o JSON malformado", body = ErrorResponse),
        (status = 401, description = "Token ausente, incorrecto o no configurado", body = ErrorResponse),
        (status = 404, description = "Organización comercial inexistente", body = ErrorResponse),
        (status = 409, description = "Instalación o etiqueta ya registrada", body = ErrorResponse),
        (status = 415, description = "Content-Type distinto de application/json", body = ErrorResponse),
        (status = 422, description = "JSON incompatible con el esquema", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn register_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<RegisterInstallationBody>, JsonRejection>,
) -> Result<(StatusCode, Json<InstallationResponse>), HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let Json(request) = request.map_err(json_error)?;
    let view = state
        .service
        .register_installation(request.into(), time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view.into())))
}

/// Consulta una instalación; el identificador no concede autorización por sí solo.
#[utoipa::path(get, path = "/api/v1/installations/{id}", operation_id = "getInstallation", tag = "Instalaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Instalación encontrada", body = InstallationResponse),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn get_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<InstallationResponse>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .get_installation(path_id(id)?)
        .await
        .map_err(err)?;
    Ok(Json(view.into()))
}

/// Activa una sola vez el trial técnico de 30 días para una instalación.
#[utoipa::path(post, path = "/api/v1/installations/{id}/trial", operation_id = "startTrial", tag = "Instalaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Trial activado", body = InstallationResponse),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 409, description = "Trial activado anteriormente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn start_trial(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<InstallationResponse>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .start_trial(path_id(id)?, time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok(Json(view.into()))
}

/// Emite una autorización de desarrollo sólo cuando el firmante DEV está habilitado.
#[utoipa::path(post, path = "/api/v1/installations/{id}/licenses", operation_id = "issueLicense", tag = "Licencias DEV",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    request_body(content = IssueLicenseBody, description = "plan: trial, local, pro o enterprise; valid_until debe ser futuro"),
    security(("devAdminToken" = [])),
    responses(
        (status = 201, description = "Autorización DEV firmada", body = IssuedLicenseResponse),
        (status = 400, description = "UUID, plan, vigencia o JSON malformado", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 415, description = "Content-Type distinto de application/json", body = ErrorResponse),
        (status = 422, description = "JSON incompatible con el esquema", body = ErrorResponse),
        (status = 503, description = "Emisión DEV o PostgreSQL indisponibles", body = ErrorResponse)))]
async fn issue_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<IssueLicenseBody>, JsonRejection>,
) -> Result<(StatusCode, Json<IssuedLicenseResponse>), HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    if !state.licensing_enabled {
        return Err(err(AppError::LicensingDisabled));
    }
    let id = path_id(id)?;
    let Json(request) = request.map_err(json_error)?;
    let view = state
        .service
        .issue_license(id, request.into(), time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view.into())))
}

/// Lista las autorizaciones registradas; para UUID desconocidos devuelve lista vacía.
#[utoipa::path(get, path = "/api/v1/installations/{id}/licenses", operation_id = "listLicenses", tag = "Licencias DEV",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Autorizaciones; lista vacía si no existen", body = [LicenseRecordResponse], example = json!([])),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn list_licenses(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<Vec<LicenseRecordResponse>>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let licenses = state
        .service
        .list_licenses(path_id(id)?)
        .await
        .map_err(err)?;
    Ok(Json(licenses.into_iter().map(Into::into).collect()))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    #[schema(example = "cuenta@example.test")]
    pub email: String,
    #[schema(example = "contraseña-sintética")]
    pub password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub email: String,
}

fn auth_error(error: platform_application::auth::AuthError) -> HttpError {
    use platform_application::auth::AuthError;
    match error {
        AuthError::InvalidCredentials => auth::error(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "credenciales inválidas",
        ),
        AuthError::RateLimited => auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiados intentos; inténtalo más tarde",
        ),
        AuthError::InvalidSession => auth::error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "sesión inválida o expirada",
        ),
        AuthError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
}

fn limiter_key(email: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        platform_application::security::hash_token(&email.trim().to_lowercase()),
    )
}

/// Inicia una sesión comercial con credenciales verificadas.
#[utoipa::path(post, path = "/api/v1/auth/login", operation_id = "login", tag = "Autenticación",
    request_body(content = LoginRequest, description = "Credenciales comerciales; la contraseña nunca se registra"),
    responses(
        (status = 204, description = "Sesión creada; establece cookie HttpOnly hiqnet_session"),
        (status = 401, description = "Credenciales inválidas, cuenta deshabilitada o bloqueada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 429, description = "Demasiados intentos", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible o verificación saturada (auth_overloaded)", body = ErrorResponse)))]
async fn login(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let key = limiter_key(&request.email);
    if auth_state.limiter.is_limited(&key) {
        return Err(auth_error(
            platform_application::auth::AuthError::RateLimited,
        ));
    }
    // Cota global de verificaciones Argon2id: responde rápido bajo saturación
    // en lugar de acumular trabajo CPU en la cola del runtime.
    let _permit = auth_state
        .argon2_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            auth::error(
                StatusCode::SERVICE_UNAVAILABLE,
                "auth_overloaded",
                "servicio de autenticación ocupado; reintenta más tarde",
            )
        })?;
    let issued = match auth_state
        .service
        .login(
            &request.email,
            &request.password,
            time::OffsetDateTime::now_utc(),
            auth_state.session_ttl,
        )
        .await
    {
        Ok(issued) => issued,
        Err(error) => {
            if matches!(
                error,
                platform_application::auth::AuthError::InvalidCredentials
            ) {
                auth_state.limiter.record(&key);
            }
            return Err(auth_error(error));
        }
    };
    auth_state.limiter.reset(&key);
    let cookie = auth::session_cookie(
        &auth_state,
        &issued.token,
        std::time::Duration::from_secs(auth_state.session_ttl.whole_seconds().max(0) as u64),
    );
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&cookie)
            .map_err(|_| auth_error(platform_application::auth::AuthError::Storage))?,
    );
    Ok(response)
}

/// Cierra la sesión actual y borra la cookie. Operación idempotente.
#[utoipa::path(post, path = "/api/v1/auth/logout", operation_id = "logout", tag = "Autenticación",
    security(("commercialSession" = [])),
    responses(
        (status = 204, description = "Sesión revocada y cookie eliminada"),
        (status = 403, description = "Origen no permitido", body = ErrorResponse)))]
async fn logout(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    if let Some(token) = auth::cookie_token(&headers) {
        auth_state
            .service
            .logout(&token, time::OffsetDateTime::now_utc())
            .await
            .map_err(auth_error)?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&auth::clear_cookie(&auth_state))
            .map_err(|_| auth_error(platform_application::auth::AuthError::Storage))?,
    );
    Ok(response)
}

/// Devuelve la identidad derivada exclusivamente de la sesión verificada.
#[utoipa::path(get, path = "/api/v1/auth/me", operation_id = "me", tag = "Autenticación",
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Identidad autenticada", body = MeResponse),
        (status = 401, description = "Sesión inválida, expirada o revocada", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
async fn me(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
) -> Result<Json<MeResponse>, HttpError> {
    let identity = auth_state
        .service
        .identity(auth_context.session.account_id)
        .await
        .map_err(auth_error)?;
    Ok(Json(MeResponse {
        account_id: identity.account_id.as_uuid(),
        email: identity.email,
    }))
}

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = open(&database_url).await?;
    let repository: SharedRepository = Arc::new(pool_repository(pool.clone()));

    let signing_key = std::env::var("HIQNET_PLATFORM_SIGNING_KEY_B64")
        .ok()
        .filter(|value| !value.is_empty());
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

    let admin_token = std::env::var("HIQNET_PLATFORM_ADMIN_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());

    let development = std::env::var("HIQNET_PLATFORM_ENV")
        .map(|value| value == "development")
        .unwrap_or(false);
    // Secure está activo por defecto; sólo puede desactivarse explícitamente en DEV.
    let cookie_secure = match std::env::var("HIQNET_PLATFORM_COOKIE_SECURE") {
        Ok(value) if value == "false" => {
            if !development {
                anyhow::bail!(
                    "HIQNET_PLATFORM_COOKIE_SECURE=false sólo se permite con HIQNET_PLATFORM_ENV=development"
                );
            }
            false
        }
        _ => true,
    };
    let allowed_origin = std::env::var("HIQNET_PLATFORM_ALLOWED_ORIGIN")
        .ok()
        .filter(|value| !value.is_empty());
    let session_hours: i64 = std::env::var("HIQNET_PLATFORM_SESSION_TTL_HOURS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8)
        .clamp(1, 168);
    let auth_concurrency: usize = std::env::var("HIQNET_PLATFORM_AUTH_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4)
        .clamp(1, 32);
    let auth_service = Arc::new(platform_application::AuthService::new(
        platform_storage_postgres::PgAuthRepository::new(pool.clone()),
    ));

    let service = Arc::new(PlatformService::new(repository, signer));
    let state = AppState {
        service,
        admin_token,
        licensing_enabled,
        db_pool: pool,
    };
    let auth_state = AuthState {
        service: auth_service,
        cookie_secure,
        allowed_origin,
        development,
        session_ttl: time::Duration::hours(session_hours),
        limiter: Arc::new(auth::LoginLimiter::new(
            10,
            std::time::Duration::from_secs(60),
        )),
        argon2_slots: Arc::new(tokio::sync::Semaphore::new(auth_concurrency)),
    };

    // La documentación sólo se expone cuando el entorno la habilita explícitamente.
    let docs_enabled =
        std::env::var("HIQNET_PLATFORM_ENABLE_DOCS").is_ok_and(|value| value == "true");
    let app = build_router(state, docs_enabled, auth_state);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "HiQNet Platform API listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, header},
    };
    use base64::Engine as _;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tower::ServiceExt;

    fn isolated_test_url() -> String {
        let url = std::env::var("TEST_DATABASE_URL").expect("base aislada requerida");
        assert!(
            url.rsplit('/')
                .next()
                .is_some_and(|database| database.contains("test") && database != "hiqnet"),
            "TEST_DATABASE_URL debe identificar una base aislada llamada test"
        );
        url
    }

    fn state(pool: PgPool) -> AppState {
        AppState {
            service: Arc::new(PlatformService::new(
                Arc::new(pool_repository(pool.clone())),
                None,
            )),
            admin_token: None,
            licensing_enabled: false,
            db_pool: pool,
        }
    }

    fn test_auth_state(pool: PgPool) -> AuthState {
        AuthState {
            service: Arc::new(platform_application::AuthService::new(
                platform_storage_postgres::PgAuthRepository::new(pool),
            )),
            cookie_secure: false,
            allowed_origin: Some("http://127.0.0.1:8080".into()),
            development: true,
            session_ttl: time::Duration::hours(1),
            limiter: Arc::new(auth::LoginLimiter::new(
                10,
                std::time::Duration::from_secs(60),
            )),
            argon2_slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }

    fn dev_state(pool: PgPool, licensing_enabled: bool) -> AppState {
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
            admin_token: Some("token-sintetico-p01".into()),
            licensing_enabled,
            db_pool: pool,
        }
    }

    async fn test_organization(pool: &PgPool, id: Uuid) {
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Organización sintética', 'active', now())",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn remove_test_organization(pool: &PgPool, id: Uuid) {
        sqlx::query("DELETE FROM organization WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn send(
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

    #[test]
    fn generated_contract_matches_versioned_snapshot_and_expected_routes() {
        let current = serde_json::to_value(openapi_document()).unwrap();
        let saved: Value =
            serde_json::from_str(include_str!("../../../openapi/openapi.json")).unwrap();
        assert_eq!(
            current, saved,
            "regenera con cargo run -p hiqnet-platform-api --bin generate-openapi"
        );
        assert_eq!(current["openapi"], "3.1.0");
        let expected = [
            ("/health", "get"),
            ("/ready", "get"),
            ("/api/v1/plans", "get"),
            ("/api/v1/installations", "post"),
            ("/api/v1/installations/{id}", "get"),
            ("/api/v1/installations/{id}/trial", "post"),
            ("/api/v1/installations/{id}/licenses", "get"),
            ("/api/v1/installations/{id}/licenses", "post"),
            ("/api/v1/auth/login", "post"),
            ("/api/v1/auth/logout", "post"),
            ("/api/v1/auth/me", "get"),
        ];
        let actual: Vec<(&str, &str)> = current["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(path, operations)| {
                operations
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(move |method| (path.as_str(), method.as_str()))
            })
            .collect();
        assert_eq!(actual.len(), expected.len());
        for (path, method) in expected {
            let operation = &current["paths"][path][method];
            assert!(operation.is_object(), "ruta {method} {path} ausente");
            assert!(operation["operationId"].is_string());
            assert!(operation["summary"].is_string());
            assert!(
                operation["responses"]
                    .as_object()
                    .is_some_and(|responses| !responses.is_empty())
            );
            if path.starts_with("/api/v1/installations") {
                assert_eq!(operation["security"], json!([{"devAdminToken": []}]));
            } else if path == "/api/v1/auth/login" {
                assert!(operation.get("security").is_none());
            } else if path == "/api/v1/auth/logout" || path == "/api/v1/auth/me" {
                assert_eq!(operation["security"], json!([{"commercialSession": []}]));
            } else {
                assert!(operation.get("security").is_none());
            }
        }
    }

    #[tokio::test]
    async fn swagger_is_opt_in_and_serves_generated_document() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        let auth_state = test_auth_state(pool.clone());
        let disabled = build_router(state(pool.clone()), false, auth_state.clone());
        assert_eq!(
            send(&disabled, Method::GET, "/docs", None, None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&disabled, Method::GET, "/api-docs/openapi.json", None, None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let enabled = build_router(state(pool), true, auth_state);
        let spec = send(&enabled, Method::GET, "/api-docs/openapi.json", None, None).await;
        assert_eq!(spec.0, StatusCode::OK);
        assert_eq!(spec.1, serde_json::to_value(openapi_document()).unwrap());
        let redirect = enabled
            .clone()
            .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(redirect.status().is_redirection());
        assert_eq!(redirect.headers()[header::LOCATION], "/docs/");
        let ui = enabled
            .oneshot(
                Request::builder()
                    .uri("/docs/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ui.status(), StatusCode::OK);
        let html = to_bytes(ui.into_body(), 1024 * 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&html).contains("Swagger UI"));
    }

    #[tokio::test]
    async fn http_routes_auth_errors_and_real_postgres_match_contract() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        let router = build_router(
            dev_state(pool.clone(), false),
            true,
            test_auth_state(pool.clone()),
        );
        let id = Uuid::new_v4();
        let org = Uuid::new_v4();
        test_organization(&pool, org).await;
        let path = format!("/api/v1/installations/{id}");
        let body = json!({"installation_id":id,"organization_id":org,"device_label":format!("fixture-{id}")});
        let token = Some("token-sintetico-p01");

        assert_eq!(
            send(&router, Method::GET, "/health", None, None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, "/ready", None, None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, "/api/v1/plans", None, None)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, &path, None, None).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&router, Method::POST, &format!("{path}/trial"), None, None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &format!("{path}/licenses"),
                None,
                Some(json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&router, Method::GET, &path, Some("incorrecto"), None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                None,
                Some(body.clone())
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );

        let registered = send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(body.clone()),
        )
        .await;
        assert_eq!(registered.0, StatusCode::CREATED);
        assert_eq!(registered.1["id"], id.to_string());
        assert_eq!(
            send(&router, Method::GET, &path, token, None).await.0,
            StatusCode::OK
        );
        let missing = send(
            &router,
            Method::GET,
            &format!("/api/v1/installations/{}", Uuid::new_v4()),
            token,
            None,
        )
        .await;
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
        assert_eq!(missing.1["code"], "not_found");
        assert_eq!(
            send(
                &router,
                Method::GET,
                "/api/v1/installations/bad-uuid",
                token,
                None
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &router,
                Method::GET,
                &format!("{path}/licenses"),
                None,
                None
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::GET,
                &format!("{path}/licenses"),
                token,
                None
            )
            .await
            .1,
            json!([])
        );

        let trial = send(&router, Method::POST, &format!("{path}/trial"), token, None).await;
        assert_eq!(trial.0, StatusCode::OK);
        assert!(trial.1["trial_ends_at"].is_string());
        assert_eq!(
            send(&router, Method::POST, &format!("{path}/trial"), token, None)
                .await
                .0,
            StatusCode::CONFLICT
        );
        let duplicate = json!({"installation_id":Uuid::new_v4(),"organization_id":org,"device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(duplicate)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let other_org = Uuid::new_v4();
        test_organization(&pool, other_org).await;
        let wrong_org = json!({"installation_id":id,"organization_id":other_org,"device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(wrong_org)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        remove_test_organization(&pool, other_org).await;
        let invalid = json!({"organization_id":org,"device_label":"   "});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(invalid)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let invalid_type = json!({"organization_id":org,"device_label":99});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(invalid_type)
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let issue = json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"});
        assert_eq!(
            send(
                &router,
                Method::POST,
                &format!("{path}/licenses"),
                token,
                Some(issue)
            )
            .await
            .0,
            StatusCode::SERVICE_UNAVAILABLE
        );

        // La fixture sólo existe en TEST_DATABASE_URL; no altera la base principal.
        sqlx::query("DELETE FROM installation WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        remove_test_organization(&pool, org).await;
    }

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

    #[tokio::test]
    async fn dev_signing_and_json_rejections_match_documented_responses() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        let router = build_router(
            dev_state(pool.clone(), true),
            false,
            test_auth_state(pool.clone()),
        );
        let id = Uuid::new_v4();
        let org = Uuid::new_v4();
        test_organization(&pool, org).await;
        let token = Some("token-sintetico-p01");
        let path = format!("/api/v1/installations/{id}/licenses");
        let registration = json!({"installation_id": id, "organization_id": org, "device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(registration)
            )
            .await
            .0,
            StatusCode::CREATED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &path,
                token,
                Some(json!({"plan":"unknown", "valid_until":"2030-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &path,
                token,
                Some(json!({"plan":"trial", "valid_until":"2000-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let issued = send(
            &router,
            Method::POST,
            &path,
            token,
            Some(json!({"plan":"trial", "valid_until":"2030-01-01T00:00:00Z"})),
        )
        .await;
        assert_eq!(issued.0, StatusCode::CREATED);
        assert!(issued.1["grant_json"].is_string());
        assert!(issued.1["signature_b64"].is_string());
        let list = send(&router, Method::GET, &path, token, None).await;
        assert_eq!(list.0, StatusCode::OK);
        assert_eq!(list.1[0]["grant_id"], issued.1["grant_id"]);
        assert!(list.1[0]["features_json"].is_string());

        let absent_type = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/installations")
                    .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(absent_type.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let malformed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/installations")
                    .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert!(
            malformed
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json")
        );
        let cross_origin = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header(header::ORIGIN, "https://otro.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            cross_origin
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );

        sqlx::query("DELETE FROM issued_license WHERE installation_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM installation WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        remove_test_organization(&pool, org).await;
    }

    #[tokio::test]
    async fn liveness_does_not_claim_database_health_when_database_is_down() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        assert_eq!(health().await.into_response().status(), StatusCode::OK);
        assert_eq!(
            ready(State(state(pool))).await.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn readiness_uses_real_postgres() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        assert_eq!(
            ready(State(state(pool))).await.into_response().status(),
            StatusCode::OK
        );
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

    async fn seed_credential(pool: &PgPool, email: &str, password: &str) {
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

    async fn send_auth(
        router: &Router,
        uri: &str,
        cookie: Option<&str>,
        origin: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = Request::builder().method(Method::POST).uri(uri);
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
        let response = router
            .clone()
            .oneshot(request.body(payload).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn session_from(headers: &HeaderMap) -> String {
        let raw = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(raw.contains("HttpOnly"));
        assert!(raw.contains("SameSite=Lax"));
        assert!(raw.contains("Path=/api"));
        raw.split(';').next().unwrap().to_owned()
    }

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

        let (status, _, _) =
            send_auth(&router, "/api/v1/auth/login", None, None, Some(body())).await;
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
}
