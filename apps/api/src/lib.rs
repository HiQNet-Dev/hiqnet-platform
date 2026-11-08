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
use platform_application::{
    AppError, PlatformService, SharedRepository, SharedSigner,
    commercial::{CommercialService, OrganizationView},
};
use platform_core::DomainError;
use platform_core::commercial::CommercialError;
use platform_core::ids::InstallationId;
use platform_storage_postgres::{PgCommercialRepository, is_ready, open, pool_repository};
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
    pub commercial: Arc<CommercialService<PgCommercialRepository>>,
    pub organization_limiter: Arc<auth::LoginLimiter>,
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
        .routes(routes!(change_password))
        .routes(routes!(revoke_sessions))
        .routes(routes!(list_organizations))
        .routes(routes!(create_organization))
        .routes(routes!(get_organization))
        .split_for_parts();
    document.info = Info::new("HiQNet Platform API", env!("CARGO_PKG_VERSION"));
    document.info.description = Some("Plano de control técnico con identidad comercial y sesiones; sin facturación, suscripciones ni sincronización productivas. El token administrativo es temporal y exclusivo de desarrollo.".into());
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
        AuthError::WeakPassword => auth::error(
            StatusCode::BAD_REQUEST,
            "weak_password",
            "la nueva contraseña no cumple la política",
        ),
        AuthError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "password_change_conflict",
            "la credencial cambió durante la operación; inicia sesión de nuevo",
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

fn commercial_error(error: CommercialError) -> HttpError {
    match error {
        CommercialError::NotFound => {
            auth::error(StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        CommercialError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "conflict",
            "el recurso ya existe o entra en conflicto",
        ),
        CommercialError::InvalidName | CommercialError::InvalidEmail => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        // Rol/versión inválidos en datos persistidos o fallo de almacenamiento.
        CommercialError::InvalidRole | CommercialError::Storage => auth::error(
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

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    #[schema(example = "contraseña-actual-larga")]
    pub current_password: String,
    #[schema(example = "nueva-contraseña-2026")]
    pub new_password: String,
}

/// Cambia la contraseña de la cuenta autenticada y rota todas sus sesiones.
#[utoipa::path(post, path = "/api/v1/auth/password/change", operation_id = "changePassword", tag = "Autenticación",
    request_body(content = ChangePasswordRequest, description = "Contraseña actual y nueva; la nueva debe tener 12–128 caracteres, al menos una letra y un dígito"),
    security(("commercialSession" = [])),
    responses(
        (status = 204, description = "Contraseña cambiada; sesiones anteriores revocadas y nueva cookie emitida"),
        (status = 400, description = "Contraseña nueva débil o igual a la actual", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o contraseña actual incorrecta", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 409, description = "Cambio concurrente detectado", body = ErrorResponse),
        (status = 429, description = "Demasiados intentos", body = ErrorResponse),
        (status = 503, description = "Almacenamiento o verificación no disponibles", body = ErrorResponse)))]
async fn change_password(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let key = format!("password-change:{}", auth_context.session.account_id);
    if auth_state.limiter.is_limited(&key) {
        return Err(auth_error(
            platform_application::auth::AuthError::RateLimited,
        ));
    }
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
        .change_password(
            auth_context.session.account_id,
            auth_context.session.session_id,
            &request.current_password,
            &request.new_password,
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

/// Revoca todas las sesiones de la cuenta autenticada, incluida la actual.
#[utoipa::path(post, path = "/api/v1/auth/sessions/revoke", operation_id = "revokeSessions", tag = "Autenticación",
    security(("commercialSession" = [])),
    responses(
        (status = 204, description = "Todas las sesiones revocadas y cookie eliminada"),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
async fn revoke_sessions(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    auth_state
        .service
        .revoke_all_sessions(
            auth_context.session.account_id,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(auth_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&auth::clear_cookie(&auth_state))
            .map_err(|_| auth_error(platform_application::auth::AuthError::Storage))?,
    );
    Ok(response)
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct OrganizationResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174010")]
    pub id: Uuid,
    #[schema(example = "Mi negocio")]
    pub display_name: String,
    #[schema(example = "owner")]
    pub role: String,
    #[schema(example = "active")]
    pub status: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct OrganizationListResponse {
    pub items: Vec<OrganizationResponse>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateOrganizationRequest {
    #[schema(example = "Mi negocio", min_length = 1, max_length = 200)]
    pub display_name: String,
}

fn organization_response(view: OrganizationView) -> OrganizationResponse {
    OrganizationResponse {
        id: view.id.as_uuid(),
        display_name: view.display_name,
        role: view.role.as_str().to_owned(),
        status: view.status.to_owned(),
    }
}

/// Lista las organizaciones activas del actor autenticado (máximo técnico 100).
#[utoipa::path(get, path = "/api/v1/organizations", operation_id = "listOrganizations", tag = "Organizaciones",
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Organizaciones activas con membresía activa; orden por nombre e id", body = OrganizationListResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn list_organizations(
    State(state): State<AppState>,
    auth_context: AuthContext,
) -> Result<Json<OrganizationListResponse>, HttpError> {
    let organizations = state
        .commercial
        .list_organizations(auth_context.session.account_id)
        .await
        .map_err(commercial_error)?;
    Ok(Json(OrganizationListResponse {
        items: organizations
            .into_iter()
            .map(organization_response)
            .collect(),
    }))
}

/// Crea una organización activa y asigna owner a la cuenta autenticada.
#[utoipa::path(post, path = "/api/v1/organizations", operation_id = "createOrganization", tag = "Organizaciones",
    request_body(content = CreateOrganizationRequest, description = "Nombre visible; la cuenta autenticada recibe owner"),
    security(("commercialSession" = [])),
    responses(
        (status = 201, description = "Organización creada; cabecera Location con la ruta del recurso", body = OrganizationResponse,
            headers(("Location" = String, description = "URL del recurso creado"))),
        (status = 400, description = "Nombre inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 409, description = "Conflicto de creación", body = ErrorResponse),
        (status = 429, description = "Demasiadas creaciones", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn create_organization(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let key = format!("organization-create:{}", auth_context.session.account_id);
    if state.organization_limiter.is_limited(&key) {
        return Err(auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiadas creaciones; inténtalo más tarde",
        ));
    }
    state.organization_limiter.record(&key);
    let created = state
        .commercial
        .create_organization(
            auth_context.session.account_id,
            &request.display_name,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    // La ventana de creación cuenta también los intentos exitosos.
    let location = format!("/api/v1/organizations/{}", created.id);
    let body = OrganizationResponse {
        id: created.id.as_uuid(),
        display_name: created.display_name,
        role: "owner".into(),
        status: "active".into(),
    };
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response.headers_mut().insert(
        axum::http::header::LOCATION,
        axum::http::HeaderValue::from_str(&location)
            .map_err(|_| commercial_error(CommercialError::Storage))?,
    );
    Ok(response)
}

/// Consulta una organización propia; un UUID ajeno responde 404 neutral.
#[utoipa::path(get, path = "/api/v1/organizations/{id}", operation_id = "getOrganization", tag = "Organizaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización")),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Organización activa con membresía activa", body = OrganizationResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 404, description = "No existe o pertenece a otra cuenta", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn get_organization(
    State(state): State<AppState>,
    auth_context: AuthContext,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<OrganizationResponse>, HttpError> {
    let Path(id) = id.map_err(path_error)?;
    let organization = state
        .commercial
        .get_organization(
            auth_context.session.account_id,
            platform_core::ids::OrganizationId::from_uuid(id),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(organization_response(organization)))
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
    let commercial = Arc::new(CommercialService::new(PgCommercialRepository::new(
        pool.clone(),
    )));
    let state = AppState {
        service,
        commercial,
        organization_limiter: Arc::new(auth::LoginLimiter::new(
            5,
            std::time::Duration::from_secs(60),
        )),
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
            commercial: Arc::new(CommercialService::new(PgCommercialRepository::new(
                pool.clone(),
            ))),
            organization_limiter: Arc::new(auth::LoginLimiter::new(
                5,
                std::time::Duration::from_secs(60),
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
            commercial: Arc::new(CommercialService::new(PgCommercialRepository::new(
                pool.clone(),
            ))),
            organization_limiter: Arc::new(auth::LoginLimiter::new(
                5,
                std::time::Duration::from_secs(60),
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
            ("/api/v1/auth/password/change", "post"),
            ("/api/v1/auth/sessions/revoke", "post"),
            ("/api/v1/organizations", "get"),
            ("/api/v1/organizations", "post"),
            ("/api/v1/organizations/{id}", "get"),
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
            } else if matches!(
                path,
                "/api/v1/auth/logout"
                    | "/api/v1/auth/me"
                    | "/api/v1/auth/password/change"
                    | "/api/v1/auth/sessions/revoke"
                    | "/api/v1/organizations"
                    | "/api/v1/organizations/{id}"
            ) {
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
    async fn auth_context_distinguishes_invalid_session_from_storage_outage() {
        let unavailable = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        let router = build_router(
            state(unavailable.clone()),
            false,
            test_auth_state(unavailable),
        );
        let outage = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(header::COOKIE, "hiqnet_session=token-sintetico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(outage.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = to_bytes(outage.into_body(), 1024 * 1024).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "storage_error");
        assert!(!body.to_string().contains("postgres://"));
        assert!(!body.to_string().contains("SELECT"));

        let organizations_outage = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/organizations")
                    .header(header::COOKIE, "hiqnet_session=token-sintetico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            organizations_outage.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        let pool = open(&isolated_test_url()).await.unwrap();
        let router = build_router(state(pool.clone()), false, test_auth_state(pool));
        let invalid = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(header::COOKIE, "hiqnet_session=token-sintetico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
        let bytes = to_bytes(invalid.into_body(), 1024 * 1024).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "unauthorized");
    }

    #[tokio::test]
    async fn password_change_rotates_cookie_and_revokes_previous_session() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("pwc-{}@example.test", Uuid::new_v4());
        let initial = "contraseña-inicial-1";
        let new_password = "nueva-clave-segura-2026";
        seed_credential(&pool, &email, initial).await;
        let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
        let origin = Some("http://127.0.0.1:8080");

        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            origin,
            Some(json!({"email": email, "password": initial})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let old_cookie = session_from(&headers);

        let (status, _, body) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&old_cookie),
            origin,
            Some(json!({"current_password": "incorrecta", "new_password": new_password})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "invalid_credentials");

        let (status, _, body) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&old_cookie),
            origin,
            Some(json!({"current_password": initial, "new_password": "corta1"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "weak_password");

        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&old_cookie),
            origin,
            Some(json!({"current_password": initial, "new_password": new_password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let new_cookie = session_from(&headers);
        assert_ne!(old_cookie, new_cookie);

        let old_me = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(header::COOKIE, &old_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(old_me.status(), StatusCode::UNAUTHORIZED);
        let new_me = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(header::COOKIE, &new_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(new_me.status(), StatusCode::OK);

        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            origin,
            Some(json!({"email": email, "password": initial})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "hash anterior invalidado");
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            origin,
            Some(json!({"email": email, "password": new_password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn session_revoke_requires_origin_and_invalidates_current_cookie() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("revoke-{}@example.test", Uuid::new_v4());
        let password = "contraseña-inicial-1";
        seed_credential(&pool, &email, password).await;
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
        let cookie = session_from(&headers);

        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/sessions/revoke",
            Some(&cookie),
            Some("https://evil.example"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/auth/sessions/revoke",
            Some(&cookie),
            origin,
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

        let (status, _, _) =
            send_auth(&router, "/api/v1/auth/sessions/revoke", None, origin, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Una cookie revocada no puede completar un cambio de contraseña.
        let (status, _, body) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&cookie),
            origin,
            Some(json!({"current_password": password, "new_password": "nueva-clave-segura-2026"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "unauthorized");
    }

    #[tokio::test]
    async fn password_change_is_rate_limited() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("pwrate-{}@example.test", Uuid::new_v4());
        let password = "contraseña-inicial-1";
        seed_credential(&pool, &email, password).await;
        let mut auth_state = test_auth_state(pool.clone());
        auth_state.limiter = Arc::new(auth::LoginLimiter::new(
            1,
            std::time::Duration::from_secs(60),
        ));
        let router = build_router(state(pool.clone()), false, auth_state);
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
        let body =
            json!({"current_password": "incorrecta", "new_password": "nueva-clave-segura-2026"});
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&cookie),
            origin,
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _, response) = send_auth(
            &router,
            "/api/v1/auth/password/change",
            Some(&cookie),
            origin,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response["code"], "rate_limited");
    }

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
