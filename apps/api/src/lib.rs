//! API HTTP de HiQNet Platform.
//!
//! Seguridad: las operaciones de instalaciones y licencias exigen un token
//! administrativo DEV (`HIQNET_PLATFORM_ADMIN_TOKEN`). Sin él, se rechazan con 401.

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
mod auth;
mod contract;
mod control;
mod control_audit;
mod control_commercial;
mod control_directory;
mod control_installations;
mod control_organization_products;
mod enrollment;
mod portal_commercial;
mod staff;
use auth::{AuthContext, AuthState};
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

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<PlatformService>,
    pub commercial: Arc<CommercialService<PgCommercialRepository>>,
    pub commercial_domain: Arc<
        platform_application::commercial_domain::CommercialService<
            platform_storage_postgres::PgCommercialDomainRepository,
        >,
    >,
    pub enrollment: Arc<EnrollmentService<PgEnrollmentRepository>>,
    pub organization_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_begin_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_proof_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_approve_limiter: Arc<auth::LoginLimiter>,
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

fn query_error(rejection: QueryRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_query",
            message: "parámetros de consulta inválidos".into(),
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
        .routes(routes!(list_members, add_member))
        .routes(routes!(change_member_role, revoke_member))
        .routes(routes!(enrollment::begin_enrollment))
        .routes(routes!(enrollment::approve_enrollment))
        .routes(routes!(enrollment::prove_enrollment))
        .routes(routes!(enrollment::list_installations))
        .routes(routes!(staff::staff_login))
        .routes(routes!(staff::staff_logout))
        .routes(routes!(staff::staff_me))
        .routes(routes!(
            control::list_control_products,
            control::create_control_product
        ))
        .routes(routes!(
            control::get_control_product,
            control::update_control_product
        ))
        .routes(routes!(
            control::list_control_projects,
            control::create_control_project
        ))
        .routes(routes!(
            control::get_control_project,
            control::update_control_project
        ))
        .routes(routes!(control_directory::list_control_organizations))
        .routes(routes!(control_directory::get_control_organization))
        .routes(routes!(control_directory::list_control_accounts))
        .routes(routes!(control_directory::get_control_account))
        .routes(routes!(control_installations::list_control_installations))
        .routes(routes!(control_installations::get_control_installation))
        .routes(routes!(control_installations::suspend_control_installation))
        .routes(routes!(
            control_installations::reactivate_control_installation
        ))
        .routes(routes!(control_installations::rehost_control_installation))
        .routes(routes!(control_audit::list_control_audit))
        .routes(routes!(control_audit::get_control_audit))
        .routes(routes!(control_audit::list_control_security_events))
        .routes(routes!(control_audit::get_control_security_event))
        .routes(routes!(control_audit::list_control_staff_auth_events))
        .routes(routes!(control_audit::get_control_staff_auth_event))
        .routes(routes!(
            control_organization_products::list_control_organization_products
        ))
        .routes(routes!(
            control_organization_products::activate_control_organization_product
        ))
        .routes(routes!(
            control_organization_products::update_control_organization_product
        ))
        .routes(routes!(control_commercial::list_control_plans))
        .routes(routes!(control_commercial::get_control_plan))
        .routes(routes!(control_commercial::create_control_plan))
        .routes(routes!(control_commercial::update_control_plan))
        .routes(routes!(control_commercial::list_control_subscriptions))
        .routes(routes!(control_commercial::get_control_subscription))
        .routes(routes!(control_commercial::create_control_subscription))
        .routes(routes!(control_commercial::suspend_control_subscription))
        .routes(routes!(control_commercial::reactivate_control_subscription))
        .routes(routes!(control_commercial::cancel_control_subscription))
        .routes(routes!(control_commercial::renew_control_subscription))
        .routes(routes!(control_commercial::get_control_entitlements))
        .routes(routes!(portal_commercial::list_organization_products))
        .routes(routes!(portal_commercial::get_organization_product))
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
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "staffSession",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "hiqnet_staff_session",
                "Cookie de sesión de personal interno emitida por /api/v1/control/auth/login.",
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
        CommercialError::Forbidden => auth::error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "rol insuficiente para la operación",
        ),
        CommercialError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "conflict",
            "el recurso ya existe o entra en conflicto",
        ),
        CommercialError::InvalidName
        | CommercialError::InvalidEmail
        | CommercialError::InvalidCursor => auth::error(
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
    /// Cursor opaco para la página siguiente; ausente en la última página.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(
        example = "eyJuIjoiTWkgbmVnb2NpbyIsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTAifQ"
    )]
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MemberResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "member")]
    pub role: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime, example = "2026-11-08T00:00:00Z")]
    pub joined_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MemberListResponse {
    pub items: Vec<MemberResponse>,
    /// Cursor opaco para la página siguiente; ausente en la última página.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(
        example = "eyJ0IjoxNzYyNTYwMDAwMDAwMDAwMDAsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTEifQ"
    )]
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MemberRoleResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "admin")]
    pub role: String,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    #[param(
        example = "eyJuIjoiTWkgbmVnb2NpbyIsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTAifQ"
    )]
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddMemberRequest {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "member")]
    pub role: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeMemberRoleRequest {
    #[schema(example = "admin")]
    pub role: String,
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

fn member_response(membership: OrganizationMembership) -> MemberResponse {
    MemberResponse {
        account_id: membership.account_id.as_uuid(),
        role: membership.role.as_str().to_owned(),
        joined_at: membership.joined_at,
    }
}

fn page_params(
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<(Option<String>, u32), HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "limit debe estar entre 1 y 100",
        ));
    }
    let cursor = query.cursor.filter(|value| !value.is_empty());
    Ok((cursor, limit))
}

/// Convierte el rol del cliente; un valor fuera de owner/admin/member es 400.
fn role_from_request(role: &str) -> Result<OrganizationRole, HttpError> {
    OrganizationRole::from_storage(role, OrganizationRole::VERSION)
        .map_err(|_| auth::error(StatusCode::BAD_REQUEST, "validation_error", "rol inválido"))
}

/// Lista las organizaciones activas del actor autenticado con paginación por cursor.
#[utoipa::path(get, path = "/api/v1/organizations", operation_id = "listOrganizations", tag = "Organizaciones",
    params(PageQuery),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Organizaciones activas con membresía activa; orden por nombre e id. Usa next_cursor para continuar", body = OrganizationListResponse),
        (status = 400, description = "Cursor o límite inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn list_organizations(
    State(state): State<AppState>,
    auth_context: AuthContext,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<OrganizationListResponse>, HttpError> {
    let (cursor, limit) = page_params(query)?;
    let page = state
        .commercial
        .list_organizations(auth_context.session.account_id, cursor.as_deref(), limit)
        .await
        .map_err(commercial_error)?;
    Ok(Json(OrganizationListResponse {
        items: page.items.into_iter().map(organization_response).collect(),
        next_cursor: page.next_cursor,
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

/// Lista las membresías activas de una organización accesible para el actor.
#[utoipa::path(get, path = "/api/v1/organizations/{id}/members", operation_id = "listOrganizationMembers", tag = "Membresías",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización"), PageQuery),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Membresías activas visibles para owner, admin o member; orden por alta y cuenta", body = MemberListResponse),
        (status = 400, description = "UUID, cursor o límite inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 404, description = "Organización inexistente, ajena o sin membresía activa", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn list_members(
    State(state): State<AppState>,
    auth_context: AuthContext,
    id: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<MemberListResponse>, HttpError> {
    let Path(id) = id.map_err(path_error)?;
    let (cursor, limit) = page_params(query)?;
    let page = state
        .commercial
        .list_members(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            cursor.as_deref(),
            limit,
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(MemberListResponse {
        items: page.items.into_iter().map(member_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

/// Agrega una cuenta activa como miembro; sólo un owner de la organización.
#[utoipa::path(post, path = "/api/v1/organizations/{id}/members", operation_id = "addOrganizationMember", tag = "Membresías",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización")),
    request_body(content = AddMemberRequest, description = "account_id de la cuenta activa y rol owner/admin/member"),
    security(("commercialSession" = [])),
    responses(
        (status = 201, description = "Membresía creada; cabecera Location con la ruta del recurso", body = MemberRoleResponse,
            headers(("Location" = String, description = "URL del recurso creado"))),
        (status = 400, description = "UUID o rol inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena, cuenta inexistente o deshabilitada", body = ErrorResponse),
        (status = 409, description = "La cuenta ya tuvo o tiene membresía en la organización", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn add_member(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<AddMemberRequest>, JsonRejection>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path(id) = id.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let role = role_from_request(&request.role)?;
    state
        .commercial
        .add_member(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(request.account_id),
            role,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    let body = MemberRoleResponse {
        account_id: request.account_id,
        role: role.as_str().to_owned(),
    };
    let location = format!("/api/v1/organizations/{id}/members/{}", request.account_id);
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response.headers_mut().insert(
        axum::http::header::LOCATION,
        axum::http::HeaderValue::from_str(&location)
            .map_err(|_| commercial_error(CommercialError::Storage))?,
    );
    Ok(response)
}

/// Cambia el rol de un miembro; sólo un owner y sin romper el último propietario.
#[utoipa::path(patch, path = "/api/v1/organizations/{id}/members/{account_id}", operation_id = "changeOrganizationMemberRole", tag = "Membresías",
    params(
        ("id" = Uuid, Path, description = "Identificador UUID de la organización"),
        ("account_id" = Uuid, Path, description = "Identificador UUID de la cuenta miembro")),
    request_body(content = ChangeMemberRoleRequest, description = "Nuevo rol owner/admin/member"),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Rol actualizado", body = MemberRoleResponse),
        (status = 400, description = "UUID o rol inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena o membresía inexistente", body = ErrorResponse),
        (status = 409, description = "Rol sin cambios o último propietario protegido", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn change_member_role(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    request: Result<Json<ChangeMemberRoleRequest>, JsonRejection>,
) -> Result<Json<MemberRoleResponse>, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path((id, account_id)) = path.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let role = role_from_request(&request.role)?;
    state
        .commercial
        .change_role(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(account_id),
            role,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(MemberRoleResponse {
        account_id,
        role: role.as_str().to_owned(),
    }))
}

/// Revoca la membresía activa; sólo un owner y sin dejar la organización sin owner.
#[utoipa::path(delete, path = "/api/v1/organizations/{id}/members/{account_id}", operation_id = "revokeOrganizationMember", tag = "Membresías",
    params(
        ("id" = Uuid, Path, description = "Identificador UUID de la organización"),
        ("account_id" = Uuid, Path, description = "Identificador UUID de la cuenta miembro")),
    security(("commercialSession" = [])),
    responses(
        (status = 204, description = "Membresía revocada; deja de conceder acceso"),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena o membresía inexistente", body = ErrorResponse),
        (status = 409, description = "Último propietario protegido", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
async fn revoke_member(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<StatusCode, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path((id, account_id)) = path.map_err(path_error)?;
    state
        .commercial
        .revoke_member(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(account_id),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(StatusCode::NO_CONTENT)
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
    // Lista separada por comas de orígenes exactos; sin comodines ni prefijos.
    let allowed_origins = std::env::var("HIQNET_PLATFORM_ALLOWED_ORIGIN")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
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
    let enrollment_ttl = std::env::var("HIQNET_PLATFORM_ENROLLMENT_TTL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(platform_core::enrollment::DEFAULT_CHALLENGE_TTL_SECONDS)
        .clamp(60, 3600);
    let verification_uri = std::env::var("HIQNET_PLATFORM_ENROLLMENT_VERIFICATION_URI")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/device".into());
    let enrollment = Arc::new(EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        enrollment_ttl,
        verification_uri,
    ));
    let enrollment_begin_rate_limit = std::env::var("HIQNET_PLATFORM_ENROLLMENT_BEGIN_RATE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(30)
        .clamp(1, 1000);
    let enrollment_proof_rate_limit = std::env::var("HIQNET_PLATFORM_ENROLLMENT_PROOF_RATE_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(60)
        .clamp(1, 1000);
    let enrollment_approve_rate_limit =
        std::env::var("HIQNET_PLATFORM_ENROLLMENT_APPROVE_RATE_LIMIT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(30)
            .clamp(1, 1000);
    let enrollment_rate_window = std::env::var("HIQNET_PLATFORM_ENROLLMENT_RATE_WINDOW_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60)
        .clamp(1, 3600);
    let state = AppState {
        service,
        commercial,
        commercial_domain: Arc::new(
            platform_application::commercial_domain::CommercialService::new(
                platform_storage_postgres::PgCommercialDomainRepository::new(pool.clone()),
            ),
        ),
        enrollment,
        organization_limiter: Arc::new(auth::LoginLimiter::new(
            5,
            std::time::Duration::from_secs(60),
        )),
        enrollment_begin_limiter: Arc::new(auth::LoginLimiter::new(
            enrollment_begin_rate_limit,
            std::time::Duration::from_secs(enrollment_rate_window),
        )),
        enrollment_proof_limiter: Arc::new(auth::LoginLimiter::new(
            enrollment_proof_rate_limit,
            std::time::Duration::from_secs(enrollment_rate_window),
        )),
        enrollment_approve_limiter: Arc::new(auth::LoginLimiter::new(
            enrollment_approve_rate_limit,
            std::time::Duration::from_secs(enrollment_rate_window),
        )),
        admin_token,
        licensing_enabled,
        db_pool: pool.clone(),
    };
    let auth_state = AuthState {
        service: auth_service,
        cookie_secure,
        allowed_origins,
        development,
        session_ttl: time::Duration::hours(session_hours),
        limiter: Arc::new(auth::LoginLimiter::new(
            10,
            std::time::Duration::from_secs(60),
        )),
        argon2_slots: Arc::new(tokio::sync::Semaphore::new(auth_concurrency)),
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
                    platform_storage_postgres::PgInstallationControlRepository::new(pool.clone()),
                ),
            ),
            limiter: Arc::new(auth::LoginLimiter::new(
                10,
                std::time::Duration::from_secs(60),
            )),
            session_ttl: time::Duration::hours(
                std::env::var("HIQNET_PLATFORM_STAFF_SESSION_TTL_HOURS")
                    .ok()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(8)
                    .clamp(1, 24),
            ),
            argon2_slots: Arc::new(tokio::sync::Semaphore::new(auth_concurrency)),
        },
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
    // ConnectInfo permite limitar begin/proof por peer real. No se confían
    // X-Forwarded-For ni Forwarded sin una lista explícita de proxies.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
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
    use platform_application::enrollment::EnrollmentError;
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

    fn test_auth_state(pool: PgPool) -> AuthState {
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
                        platform_storage_postgres::PgOrganizationProductRepository::new(
                            pool.clone(),
                        ),
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

    async fn product_project_audit_count(pool: &PgPool, email: &str) -> i64 {
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
            ("/api/v1/organizations/{id}/members", "get"),
            ("/api/v1/organizations/{id}/members", "post"),
            ("/api/v1/organizations/{id}/members/{account_id}", "patch"),
            ("/api/v1/organizations/{id}/members/{account_id}", "delete"),
            ("/api/v1/enrollment-requests", "post"),
            ("/api/v1/enrollment-requests/{user_code}/approve", "post"),
            ("/api/v1/enrollment-requests/{device_code}/proof", "post"),
            ("/api/v1/organizations/{id}/installations", "get"),
            ("/api/v1/control/auth/login", "post"),
            ("/api/v1/control/auth/logout", "post"),
            ("/api/v1/control/auth/me", "get"),
            ("/api/v1/control/products", "get"),
            ("/api/v1/control/products", "post"),
            ("/api/v1/control/products/{key}", "get"),
            ("/api/v1/control/products/{key}", "patch"),
            ("/api/v1/control/projects", "get"),
            ("/api/v1/control/projects", "post"),
            ("/api/v1/control/projects/{key}", "get"),
            ("/api/v1/control/projects/{key}", "patch"),
            ("/api/v1/control/organizations", "get"),
            ("/api/v1/control/organizations/{organization_id}", "get"),
            ("/api/v1/control/accounts", "get"),
            ("/api/v1/control/accounts/{account_id}", "get"),
            ("/api/v1/control/installations", "get"),
            ("/api/v1/control/installations/{installation_id}", "get"),
            (
                "/api/v1/control/installations/{installation_id}/suspend",
                "post",
            ),
            (
                "/api/v1/control/installations/{installation_id}/reactivate",
                "post",
            ),
            (
                "/api/v1/control/installations/{installation_id}/rehost",
                "post",
            ),
            ("/api/v1/control/audit", "get"),
            ("/api/v1/control/audit/{event_id}", "get"),
            ("/api/v1/control/security-events", "get"),
            ("/api/v1/control/security-events/{event_id}", "get"),
            ("/api/v1/control/staff-auth-events", "get"),
            ("/api/v1/control/staff-auth-events/{event_id}", "get"),
            (
                "/api/v1/control/organizations/{organization_id}/products",
                "get",
            ),
            (
                "/api/v1/control/organizations/{organization_id}/products",
                "post",
            ),
            (
                "/api/v1/control/organizations/{organization_id}/products/{product_id}",
                "patch",
            ),
            ("/api/v1/control/plans", "get"),
            ("/api/v1/control/plans", "post"),
            ("/api/v1/control/plans/{plan_id}", "get"),
            ("/api/v1/control/plans/{plan_id}", "patch"),
            ("/api/v1/control/subscriptions", "get"),
            ("/api/v1/control/subscriptions", "post"),
            ("/api/v1/control/subscriptions/{subscription_id}", "get"),
            (
                "/api/v1/control/subscriptions/{subscription_id}/suspend",
                "post",
            ),
            (
                "/api/v1/control/subscriptions/{subscription_id}/reactivate",
                "post",
            ),
            (
                "/api/v1/control/subscriptions/{subscription_id}/cancel",
                "post",
            ),
            (
                "/api/v1/control/subscriptions/{subscription_id}/renew",
                "post",
            ),
            (
                "/api/v1/control/organizations/{organization_id}/entitlements",
                "get",
            ),
            ("/api/v1/organizations/{organization_id}/products", "get"),
            (
                "/api/v1/organizations/{organization_id}/products/{product_id}",
                "get",
            ),
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
            } else if path == "/api/v1/control/auth/login" {
                assert!(operation.get("security").is_none());
            } else if path.starts_with("/api/v1/control/") {
                assert_eq!(operation["security"], json!([{"staffSession": []}]));
            } else if path == "/api/v1/auth/login"
                || path == "/api/v1/enrollment-requests"
                || path.ends_with("/proof")
            {
                assert!(operation.get("security").is_none());
            } else if matches!(
                path,
                "/api/v1/auth/logout"
                    | "/api/v1/auth/me"
                    | "/api/v1/auth/password/change"
                    | "/api/v1/auth/sessions/revoke"
                    | "/api/v1/enrollment-requests/{user_code}/approve"
            ) || path.starts_with("/api/v1/organizations")
            {
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
        // Desde 0009 el historial de instalaciones es inmutable: no se limpia con DELETE.
        assert!(
            sqlx::query("DELETE FROM installation WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .is_err(),
            "la instalación no admite eliminación física"
        );
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
        // Desde 0009 el historial de instalaciones es inmutable: no se limpia con DELETE.
        assert!(
            sqlx::query("DELETE FROM installation WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .is_err(),
            "la instalación no admite eliminación física"
        );
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
    fn enrollment_invalid_maps_to_conflict_not_forbidden() {
        let (status, body) = enrollment::enrollment_error(EnrollmentError::EnrollmentInvalid);
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
        send_method(router, Method::POST, uri, cookie, origin, body).await
    }

    async fn send_method(
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

    async fn send_method_from(
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
            .clone()
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

        let members_outage = router
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/organizations/{}/members", Uuid::new_v4()))
                    .header(header::COOKIE, "hiqnet_session=token-sintetico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(members_outage.status(), StatusCode::SERVICE_UNAVAILABLE);

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
        let (status, _, _) =
            send_method(&router, Method::GET, &members_uri, None, None, None).await;
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

    #[tokio::test]
    async fn enrollment_rate_limiters_are_scoped_by_peer_and_account() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email_a = format!("rate-a-{}@example.test", Uuid::new_v4());
        let email_b = format!("rate-b-{}@example.test", Uuid::new_v4());
        let password = "contraseña-inicial-1";
        seed_credential(&pool, &email_a, password).await;
        seed_credential(&pool, &email_b, password).await;
        let mut app_state = state(pool.clone());
        app_state.enrollment_begin_limiter = Arc::new(auth::LoginLimiter::new(
            1,
            std::time::Duration::from_secs(60),
        ));
        app_state.enrollment_proof_limiter = Arc::new(auth::LoginLimiter::new(
            1,
            std::time::Duration::from_secs(60),
        ));
        app_state.enrollment_approve_limiter = Arc::new(auth::LoginLimiter::new(
            1,
            std::time::Duration::from_secs(60),
        ));
        let router = build_router(app_state, false, test_auth_state(pool.clone()));
        let product_key = format!("rate-pos-{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, 'Rate product', '', 'active', 'desktop', now(), now())",
        )
        .bind(Uuid::new_v4())
        .bind(&product_key)
        .execute(&pool)
        .await
        .unwrap();
        let peer_a = std::net::SocketAddr::from(([127, 0, 0, 2], 41001));
        let peer_b = std::net::SocketAddr::from(([127, 0, 0, 3], 41002));
        let origin = Some("http://127.0.0.1:8080");
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
        let public_key = format!(
            "ed25519:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing_key.verifying_key().as_bytes())
        );
        let body = |label: &str| {
            json!({
                "installation_id": Uuid::new_v4(),
                "product_key": product_key,
                "public_key": public_key.clone(),
                "device_label": label,
                "app_version": "0.1.0",
                "platform": "linux"
            })
        };
        let (status, _, _) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests",
            None,
            None,
            Some(body("Caja rate A")),
            peer_a,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _, response) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests",
            None,
            None,
            Some(body("Caja rate A2")),
            peer_a,
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response["code"], "rate_limited");
        let (status, _, _) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests",
            None,
            None,
            Some(body("Caja rate B")),
            peer_b,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        let proof_body = json!({
            "installation_id": Uuid::new_v4(),
            "public_key": public_key,
            "nonce": "nonce-rate",
            "signature": "firma-rate"
        });
        let (status, _, _) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/device-rate/proof",
            None,
            None,
            Some(proof_body.clone()),
            peer_a,
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
        let (status, _, _) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/device-rate/proof",
            None,
            None,
            Some(proof_body.clone()),
            peer_a,
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        let (status, _, _) = send_method_from(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/device-rate/proof",
            None,
            None,
            Some(proof_body),
            peer_b,
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);

        let login = |email: String| {
            let router = router.clone();
            async move {
                let (status, headers, _) = send_auth(
                    &router,
                    "/api/v1/auth/login",
                    None,
                    Some("http://127.0.0.1:8080"),
                    Some(json!({"email": email, "password": password})),
                )
                .await;
                assert_eq!(status, StatusCode::NO_CONTENT);
                session_from(&headers)
            }
        };
        let cookie_a = login(email_a).await;
        let cookie_b = login(email_b).await;
        let approve_body = json!({"organization_id": Uuid::new_v4()});
        let (status, _, _) = send_method(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/AAAA-BBBB/approve",
            Some(&cookie_a),
            origin,
            Some(approve_body.clone()),
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
        let (status, _, _) = send_method(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/AAAA-BBBB/approve",
            Some(&cookie_a),
            origin,
            Some(approve_body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        let (status, _, _) = send_method(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests/AAAA-BBBB/approve",
            Some(&cookie_b),
            origin,
            Some(approve_body),
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn enrollment_http_flow_rbac_and_cross_tenant() {
        use ed25519_dalek::Signer as _;
        use platform_core::enrollment::canonical_proof_message;

        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("enroll-{}@example.test", Uuid::new_v4());
        let other_email = format!("other-{}@example.test", Uuid::new_v4());
        let password = "contraseña-inicial-1";
        seed_credential(&pool, &email, password).await;
        seed_credential(&pool, &other_email, password).await;
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
        let owner_cookie = session_from(&headers);

        let (status, _, organization) = send_method(
            &router,
            Method::POST,
            "/api/v1/organizations",
            Some(&owner_cookie),
            origin,
            Some(json!({"display_name": "Org enrollment HTTP"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let organization_id = organization["id"].as_str().unwrap().to_owned();
        let product_key = format!("http-pos-{}", Uuid::new_v4().simple());
        let product_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, 'HTTP product', '', 'active', 'desktop', now(), now())",
        )
        .bind(product_id)
        .bind(&product_key)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
             VALUES ($1, $2, 'active', now())",
        )
        .bind(Uuid::parse_str(&organization_id).unwrap())
        .bind(product_id)
        .execute(&pool)
        .await
        .unwrap();

        let installation_id = Uuid::new_v4();
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
        let public_key = format!(
            "ed25519:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing_key.verifying_key().as_bytes())
        );
        let (status, _, challenge) = send_method(
            &router,
            Method::POST,
            "/api/v1/enrollment-requests",
            None,
            None,
            Some(json!({
                "installation_id": installation_id,
                "product_key": product_key,
                "public_key": public_key,
                "device_label": "Caja HTTP",
                "app_version": "0.1.0",
                "platform": "windows"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let device_code = challenge["device_code"].as_str().unwrap().to_owned();
        let user_code = challenge["user_code"].as_str().unwrap().to_owned();
        let nonce = challenge["nonce"].as_str().unwrap().to_owned();
        assert_eq!(challenge["expires_in"], 600);

        let (status, _, _) = send_auth(
            &router,
            &format!("/api/v1/enrollment-requests/{user_code}/approve"),
            Some(&owner_cookie),
            Some("http://evil.example"),
            Some(json!({"organization_id": organization_id})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            "/api/v1/installations/123e4567-e89b-42d3-a456-4266141740b2",
            Some(&owner_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "una cookie comercial no sustituye devAdminToken"
        );
        let (status, _, approved) = send_auth(
            &router,
            &format!("/api/v1/enrollment-requests/{user_code}/approve"),
            Some(&owner_cookie),
            origin,
            Some(json!({"organization_id": organization_id})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(approved["status"], "approved");

        let message = canonical_proof_message(
            &device_code,
            InstallationId::from_uuid(installation_id),
            &public_key,
            &nonce,
        );
        let signature = signing_key.sign(&message);
        let proof = json!({
            "installation_id": installation_id,
            "public_key": public_key,
            "nonce": nonce,
            "signature": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())
        });
        let (status, _, completed) = send_method(
            &router,
            Method::POST,
            &format!("/api/v1/enrollment-requests/{device_code}/proof"),
            None,
            None,
            Some(proof.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(completed["installation_id"], installation_id.to_string());

        let (status, _, list) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{organization_id}/installations"),
            Some(&owner_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            list["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["installation_id"] == installation_id.to_string())
        );

        let (status, _, replay) = send_method(
            &router,
            Method::POST,
            &format!("/api/v1/enrollment-requests/{device_code}/proof"),
            None,
            None,
            Some(proof),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(replay["installation_id"], installation_id.to_string());

        let (status, other_headers, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            origin,
            Some(json!({"email": other_email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let other_cookie = session_from(&other_headers);
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{organization_id}/installations"),
            Some(&other_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    async fn seed_staff(
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

    fn staff_session_from(headers: &HeaderMap) -> String {
        let raw = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(raw.contains("HttpOnly"));
        assert!(raw.contains("SameSite=Strict"));
        assert!(raw.contains("Path=/api/v1/control"));
        assert!(raw.starts_with("hiqnet_staff_session="));
        raw.split(';').next().unwrap().to_owned()
    }

    #[tokio::test]
    async fn staff_auth_is_separate_and_revocable() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("staff-{}@hiqnet.test", Uuid::new_v4());
        let password = "contraseña-staff-larga";
        seed_staff(
            &pool,
            &email,
            password,
            platform_core::staff::StaffRole::SuperAdmin,
        )
        .await;
        let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));

        // Origen ajeno rechazado antes de tocar credenciales.
        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/control/auth/login",
            None,
            Some("https://evil.example"),
            Some(json!({"email": email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(headers.get(header::SET_COOKIE).is_none());

        // Login staff correcto: cookie propia, SameSite=Strict, Path del Control Center.
        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/control/auth/login",
            None,
            Some("http://127.0.0.1:8080"),
            Some(json!({"email": email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let staff_cookie = staff_session_from(&headers);

        // /me deriva identidad y capabilities de la sesión verificada.
        let (status, _, body) = send_method(
            &router,
            Method::GET,
            "/api/v1/control/auth/me",
            Some(&staff_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["role"], "super_admin");
        assert!(
            body["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|capability| capability.as_str() == Some("staff.manage"))
        );

        // El token DEV no concede acceso staff.
        let mut request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/control/auth/me")
            .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                40000,
            ))));
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Una cookie comercial no accede al Control Center.
        let commercial_email = format!("api-{}@example.test", Uuid::new_v4());
        seed_credential(&pool, &commercial_email, "contraseña-api-larga").await;
        let (status, commercial_headers, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            Some("http://127.0.0.1:8080"),
            Some(json!({"email": commercial_email, "password": "contraseña-api-larga"})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let commercial_cookie = session_from(&commercial_headers);
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            "/api/v1/control/auth/me",
            Some(&commercial_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Logout revoca la sesión staff.
        let (status, _, _) = send_auth(
            &router,
            "/api/v1/control/auth/logout",
            Some(&staff_cookie),
            Some("http://127.0.0.1:8080"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            "/api/v1/control/auth/me",
            Some(&staff_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Deshabilitar la cuenta invalida de inmediato cualquier sesión nueva.
        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/control/auth/login",
            None,
            Some("http://127.0.0.1:8080"),
            Some(json!({"email": email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let active_cookie = staff_session_from(&headers);
        sqlx::query("UPDATE staff_account SET status = 'disabled' WHERE email = $1")
            .bind(&email)
            .execute(&pool)
            .await
            .unwrap();
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            "/api/v1/control/auth/me",
            Some(&active_cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

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

    fn assert_rfc3339(value: &Value, field: &str) {
        let text = value
            .as_str()
            .unwrap_or_else(|| panic!("{field} debe ser string RFC3339: {value}"));
        time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|error| panic!("{field} no parsea como RFC3339: {text}: {error}"));
    }

    async fn seed_control_fixture(pool: &PgPool) -> (String, Uuid) {
        let prefix = format!("c4-{}", Uuid::new_v4().simple());
        let timestamp = time::OffsetDateTime::now_utc();
        for (index, status) in ["active", "legacy_unclaimed", "archived"]
            .iter()
            .enumerate()
        {
            sqlx::query(
                "INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, $2, $3, $4)",
            )
            .bind(Uuid::new_v4())
            .bind(format!("{prefix}-org-{index}"))
            .bind(*status)
            .bind(timestamp)
            .execute(pool)
            .await
            .unwrap();
        }
        let organization_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, $2, 'active', $3)",
        )
        .bind(organization_id)
        .bind(format!("{prefix}-detalle"))
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
        let first_account = Uuid::new_v4();
        let revoked_account = Uuid::new_v4();
        for (id, email, name, status) in [
            (
                first_account,
                format!("{prefix}-first@example.test"),
                "Primera Cuenta".to_owned(),
                "active",
            ),
            (
                revoked_account,
                format!("{prefix}-revoked@example.test"),
                "Cuenta Revocada".to_owned(),
                "disabled",
            ),
        ] {
            sqlx::query(
                "INSERT INTO commercial_account (id, email, display_name, status, created_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(email)
            .bind(name)
            .bind(status)
            .bind(timestamp)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, 'owner', 1, $3)",
        )
        .bind(organization_id)
        .bind(first_account)
        .bind(timestamp)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at, revoked_at)
             VALUES ($1, $2, 'member', 1, $3, $4)",
        )
        .bind(organization_id)
        .bind(revoked_account)
        .bind(timestamp + time::Duration::seconds(1))
        .bind(timestamp + time::Duration::seconds(2))
        .execute(pool)
        .await
        .unwrap();
        for (status, association) in [("active", "active"), ("retired", "inactive")] {
            let product_id = Uuid::new_v4();
            let key = format!("{prefix}-product-{status}");
            sqlx::query(
                "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
                 VALUES ($1, $2, $3, '', $4, 'saas', now(), now())",
            )
            .bind(product_id)
            .bind(&key)
            .bind(format!("Product {key}"))
            .bind(status)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
                 VALUES ($1, $2, $3, now())",
            )
            .bind(organization_id)
            .bind(product_id)
            .bind(association)
            .execute(pool)
            .await
            .unwrap();
        }
        (prefix, organization_id)
    }

    #[tokio::test]
    async fn control_directory_paginates_filters_and_preserves_history() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("staff-directory-{}@hiqnet.test", Uuid::new_v4());
        let password = "contraseña-staff-larga";
        seed_staff(
            &pool,
            &email,
            password,
            platform_core::staff::StaffRole::ReadOnly,
        )
        .await;
        let (prefix, organization_id) = seed_control_fixture(&pool).await;
        let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
        let origin = "http://127.0.0.1:8080";
        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/control/auth/login",
            None,
            Some(origin),
            Some(json!({"email": email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let cookie = staff_session_from(&headers);
        let audit_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_audit_event e
             JOIN staff_account a ON a.id = e.staff_account_id WHERE a.email = $1",
        )
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations?search={prefix}&limit=2"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_rfc3339(&body["items"][0]["created_at"], "organization.created_at");
        let next_cursor = body["next_cursor"].as_str().unwrap().to_owned();

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations?search={prefix}&limit=2&cursor={next_cursor}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["items"].as_array().unwrap().len(), 2);

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations?status=archived&search={prefix}"),
            Some(&cookie),
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
                .all(|item| item["status"] == "archived")
        );

        for query in [
            "status=suspended".to_owned(),
            "cursor=no-es-un-cursor".to_owned(),
            format!("search={}", "x".repeat(201)),
        ] {
            let (status, _, body) = send_method(
                &router,
                Method::GET,
                &format!("/api/v1/control/organizations?{query}"),
                Some(&cookie),
                None,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "query {query}");
            assert_eq!(body["code"], "validation_error");
        }

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations/{organization_id}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["memberships"].as_array().unwrap().len(), 2);
        for membership in body["memberships"].as_array().unwrap() {
            assert_rfc3339(&membership["joined_at"], "membership.joined_at");
            if !membership["revoked_at"].is_null() {
                assert_rfc3339(&membership["revoked_at"], "membership.revoked_at");
            }
        }
        assert!(
            body["memberships"]
                .as_array()
                .unwrap()
                .iter()
                .any(|membership| !membership["revoked_at"].is_null()),
            "la membership revocada se conserva"
        );
        for product in body["products"].as_array().unwrap() {
            assert_rfc3339(
                &product["activated_at"],
                "organization_product.activated_at",
            );
        }
        assert_eq!(body["products"].as_array().unwrap().len(), 2);
        assert!(
            body["products"]
                .as_array()
                .unwrap()
                .iter()
                .any(|product| product["association_status"] == "inactive")
        );

        let (status, _, _) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/organizations/{}", Uuid::new_v4()),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = send_method(
            &router,
            Method::GET,
            "/api/v1/control/organizations/no-es-uuid",
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/accounts?search={prefix}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_rfc3339(&body["items"][0]["created_at"], "account.created_at");
        let account_id = body["items"][0]["id"].as_str().unwrap().to_owned();

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/accounts/{account_id}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["memberships"].as_array().unwrap().len(), 1);
        assert_rfc3339(
            &body["memberships"][0]["joined_at"],
            "account_membership.joined_at",
        );

        let (status, _, body) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/accounts?status=disabled&search={prefix}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["items"].as_array().unwrap().len(), 1);

        let audit_after: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_audit_event e
             JOIN staff_account a ON a.id = e.staff_account_id WHERE a.email = $1",
        )
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_before, audit_after, "las lecturas no auditan");
    }

    #[tokio::test]
    async fn control_directory_rejects_invalid_query_bounds() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("staff-bounds-{}@hiqnet.test", Uuid::new_v4());
        let password = "contraseña-staff-larga";
        seed_staff(
            &pool,
            &email,
            password,
            platform_core::staff::StaffRole::ReadOnly,
        )
        .await;
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

        for path in ["/api/v1/control/organizations", "/api/v1/control/accounts"] {
            for query in ["limit=0", "limit=101", "limit=no-es-numero"] {
                let (status, _, body) = send_method(
                    &router,
                    Method::GET,
                    &format!("{path}?{query}"),
                    Some(&cookie),
                    None,
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?{query}");
                assert_eq!(body["code"], "validation_error", "{path}?{query}");
            }
        }
    }

    #[test]
    fn control_directory_openapi_exposes_closed_enums() {
        let document = serde_json::to_value(openapi_document()).unwrap();
        for (schema, expected) in [
            ("OrganizationStatusDto", "legacy_unclaimed"),
            ("AccountStatusDto", "disabled"),
            ("MembershipRoleDto", "owner"),
            ("AssociationStatusDto", "inactive"),
        ] {
            let values = document["components"]["schemas"][schema]["enum"]
                .as_array()
                .unwrap_or_else(|| panic!("schema {schema}"));
            assert!(
                values.iter().any(|value| value == expected),
                "{schema} debe publicar {expected}"
            );
        }
    }

    fn random_bytes32() -> Vec<u8> {
        let mut bytes = Uuid::new_v4().as_bytes().to_vec();
        bytes.extend_from_slice(Uuid::new_v4().as_bytes());
        bytes
    }

    async fn control_login(router: &Router, email: &str, password: &str) -> String {
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

    #[tokio::test]
    async fn control_installations_require_staff_capability_and_cover_contract() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let router = build_router(state(pool.clone()), true, test_auth_state(pool.clone()));

        let organization = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at) \
             VALUES ($1, $2, 'active', now())",
        )
        .bind(organization)
        .bind(format!("API C5 {organization}"))
        .execute(&pool)
        .await
        .unwrap();
        let product = Uuid::new_v4();
        let product_key = format!("api-c5-{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO product \
             (id, key, name, description, status, product_type, created_at, updated_at) \
             VALUES ($1, $2, $3, '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(&product_key)
        .bind(format!("Product {product_key}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at) \
             VALUES ($1, $2, 'active', now())",
        )
        .bind(organization)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
        let owner = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO commercial_account (id, email, display_name, created_at) \
             VALUES ($1, $2, 'Owner API C5', now())",
        )
        .bind(owner)
        .bind(format!("api-c5-owner-{owner}@example.test"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_membership \
             (organization_id, account_id, role, role_version, joined_at) \
             VALUES ($1, $2, 'owner', 1, now())",
        )
        .bind(organization)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();

        let installation = Uuid::new_v4();
        let device_label = format!("API C5 caja {}", &installation.to_string()[..8]);
        sqlx::query(
            "INSERT INTO installation \
             (id, organization_id, product_id, device_label, status, identity_status, \
              registered_at, enrolled_at) \
             VALUES ($1, $2, $3, $4, 'active', 'active', now(), now())",
        )
        .bind(installation)
        .bind(organization)
        .bind(product)
        .bind(&device_label)
        .execute(&pool)
        .await
        .unwrap();
        let identity = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO installation_identity \
             (id, installation_id, public_key, fingerprint, generation, created_at) \
             VALUES ($1, $2, $3, $4, 1, now())",
        )
        .bind(identity)
        .bind(installation)
        .bind(random_bytes32())
        .bind(random_bytes32())
        .execute(&pool)
        .await
        .unwrap();

        let insert_request = |status: &'static str| {
            let consumed_at = (status == "completed").then(time::OffsetDateTime::now_utc);
            sqlx::query(
                "INSERT INTO enrollment_request \
                 (id, installation_id, product_id, public_key, fingerprint, device_label, \
                  app_version, platform, device_code_hash, user_code_hash, nonce_hash, status, \
                  organization_id, approved_by_account_id, idempotency_scope, idempotency_key, \
                  request_hash, created_at, expires_at, approved_at, consumed_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, '0.1.0', 'windows', $7, $8, $9, $10, \
                         $11, $12, 'api-c5', NULL, $13, now(), now() + interval '30 minutes', \
                         now(), $14)",
            )
            .bind(Uuid::new_v4())
            .bind(installation)
            .bind(product)
            .bind(random_bytes32())
            .bind(random_bytes32())
            .bind(format!("API C5 request {}", Uuid::new_v4()))
            .bind(random_bytes32())
            .bind(random_bytes32())
            .bind(random_bytes32())
            .bind(status)
            .bind(organization)
            .bind(owner)
            .bind(random_bytes32())
            .bind(consumed_at)
        };
        insert_request("completed").execute(&pool).await.unwrap();
        insert_request("approved").execute(&pool).await.unwrap();
        let original: Uuid = sqlx::query_scalar(
            "SELECT id FROM enrollment_request WHERE installation_id = $1 AND status = 'completed'",
        )
        .bind(installation)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO installation_binding \
             (id, organization_id, installation_id, enrollment_request_id, \
              installation_identity_id, approved_by_account_id, bound_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now())",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(installation)
        .bind(original)
        .bind(identity)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
        let rehost_request: Uuid = sqlx::query_scalar(
            "SELECT id FROM enrollment_request WHERE installation_id = $1 AND status = 'approved'",
        )
        .bind(installation)
        .fetch_one(&pool)
        .await
        .unwrap();

        let password = "contraseña-staff-larga";
        let reader_email = format!("c5-reader-{}@hiqnet.test", Uuid::new_v4());
        let manager_email = format!("c5-manager-{}@hiqnet.test", Uuid::new_v4());
        let admin_email = format!("c5-admin-{}@hiqnet.test", Uuid::new_v4());
        seed_staff(
            &pool,
            &reader_email,
            password,
            platform_core::staff::StaffRole::Support,
        )
        .await;
        seed_staff(
            &pool,
            &manager_email,
            password,
            platform_core::staff::StaffRole::Operator,
        )
        .await;
        seed_staff(
            &pool,
            &admin_email,
            password,
            platform_core::staff::StaffRole::PlatformAdmin,
        )
        .await;

        let origin = Some("http://127.0.0.1:8080");
        let list_path =
            format!("/api/v1/control/installations?organization_id={organization}&limit=10");
        let (status, _, _) = send_method(&router, Method::GET, &list_path, None, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "sin sesión staff");

        let mut dev_request = Request::builder()
            .method(Method::GET)
            .uri(&list_path)
            .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
            .body(Body::empty())
            .unwrap();
        dev_request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                40000,
            ))));
        let response = router.clone().oneshot(dev_request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "devAdminToken no accede a Control"
        );

        let reader = control_login(&router, &reader_email, password).await;
        let manager = control_login(&router, &manager_email, password).await;
        let admin = control_login(&router, &admin_email, password).await;

        let (status, _, body) =
            send_method(&router, Method::GET, &list_path, Some(&reader), None, None).await;
        assert_eq!(status, StatusCode::OK);
        let items = body["items"].as_array().unwrap();
        assert!(items.iter().any(|item| item["id"] == json!(installation)));
        let registered_at = items[0]["registered_at"].as_str().unwrap();
        assert!(
            time::OffsetDateTime::parse(
                registered_at,
                &time::format_description::well_known::Rfc3339
            )
            .is_ok(),
            "registered_at debe ser RFC3339: {registered_at}"
        );

        let (status, _, detail) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/installations/{installation}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["summary"]["id"], json!(installation));
        assert_eq!(detail["summary"]["product"]["key"], json!(product_key));
        assert_eq!(
            detail["current_binding"]["approved_by_account_id"],
            json!(owner)
        );
        let identity_created_at = detail["current_identity_created_at"]
            .as_str()
            .expect("current_identity_created_at debe ser RFC3339");
        time::OffsetDateTime::parse(
            identity_created_at,
            &time::format_description::well_known::Rfc3339,
        )
        .expect("current_identity_created_at no parsea como RFC3339");

        for (path, expected_status, expected_code) in [
            (
                format!("/api/v1/control/installations/{}", Uuid::new_v4()),
                StatusCode::NOT_FOUND,
                "not_found",
            ),
            (
                "/api/v1/control/installations/no-es-uuid".into(),
                StatusCode::BAD_REQUEST,
                "invalid_path",
            ),
        ] {
            let (status, _, error) =
                send_method(&router, Method::GET, &path, Some(&reader), None, None).await;
            assert_eq!(status, expected_status, "{path}");
            assert_eq!(error["code"], expected_code);
        }

        for query in ["limit=0", "limit=101", "limit=abc", "cursor=no-es-cursor"] {
            let (status, _, error) = send_method(
                &router,
                Method::GET,
                &format!("/api/v1/control/installations?{query}"),
                Some(&reader),
                None,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
            assert_eq!(error["code"], "validation_error", "{query}");
        }
        for query in ["status=bogus", "identity_status=bogus"] {
            let (status, _, error) = send_method(
                &router,
                Method::GET,
                &format!("/api/v1/control/installations?{query}"),
                Some(&reader),
                None,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
            assert_eq!(error["code"], "validation_error", "{query}");
        }
        let (status, _, error) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/installations?search={}", "x".repeat(201)),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["code"], "validation_error");

        let suspend_path = format!("/api/v1/control/installations/{installation}/suspend");
        let (status, _, _) = send_method(
            &router,
            Method::POST,
            &suspend_path,
            Some(&reader),
            origin,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "read sin manage");

        let (status, _, suspended) = send_method(
            &router,
            Method::POST,
            &suspend_path,
            Some(&manager),
            origin,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            suspended["summary"]["administrative_status"],
            json!("suspended")
        );
        let (status, _, reactivated) = send_method(
            &router,
            Method::POST,
            &format!("/api/v1/control/installations/{installation}/reactivate"),
            Some(&manager),
            origin,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            reactivated["summary"]["administrative_status"],
            json!("active")
        );

        let rehost_path = format!("/api/v1/control/installations/{installation}/rehost");
        let rehost_body = json!({"enrollment_request_id": rehost_request, "reason": "reemplazo"});
        let (status, _, error) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&manager),
            origin,
            Some(rehost_body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "manage sin rehost");
        assert_eq!(error["code"], "forbidden");

        let (status, _, error) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&admin),
            Some("https://evil.example"),
            Some(rehost_body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(error["code"], "origin_not_allowed");

        let (status, _, _) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&admin),
            origin,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _, error) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&admin),
            origin,
            Some(json!({"enrollment_request_id": rehost_request, "reason": "   "})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["code"], "validation_error");

        let (status, _, error) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&admin),
            origin,
            Some(json!({"enrollment_request_id": Uuid::new_v4(), "reason": "ajena"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error["code"], "rehost_conflict");

        let (status, _, authorization) = send_method(
            &router,
            Method::POST,
            &rehost_path,
            Some(&admin),
            origin,
            Some(rehost_body),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(authorization["status"], json!("pending"));
        assert_eq!(
            authorization["enrollment_request_id"],
            json!(rehost_request)
        );

        let (status, _, detail) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/installations/{installation}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["summary"]["identity_status"], json!("unenrolled"));
        assert_eq!(detail["summary"]["binding_active"], json!(false));
        assert!(detail["current_binding"].is_null());
        assert!(detail["current_identity_created_at"].is_null());
        assert_eq!(detail["rehost"]["status"], json!("pending"));
    }

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

    #[tokio::test]
    async fn commercial_plans_subscriptions_and_entitlements_cover_contract() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let admin_email = format!("commercial-admin-{}@hiqnet.test", Uuid::new_v4());
        let reader_email = format!("commercial-reader-{}@hiqnet.test", Uuid::new_v4());
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
             VALUES ($1, 'Commercial API org', 'active', now())",
        )
        .bind(organization)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, 'Commercial API product', '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(format!("commercial-api-{}", Uuid::new_v4().simple()))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, 'active', now())",
        )
        .bind(organization)
        .bind(product)
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

        let (status, _, plan) = send_method(
            &router,
            Method::POST,
            "/api/v1/control/plans",
            Some(&admin),
            origin,
            Some(json!({
                "product_id": product,
                "key": "pro",
                "name": "Pro",
                "status": "active",
                "features": {"sync": true},
                "limits": {"seats": 10}
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let plan_id: Uuid = serde_json::from_value(plan["id"].clone()).unwrap();

        let (status, _, plans) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/plans?product_id={product}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(plans["items"].as_array().unwrap().len(), 1);

        let subscription_body = json!({
            "organization_id": organization,
            "product_id": product,
            "plan_id": plan_id,
            "starts_at": "2026-01-01T00:00:00Z",
            "ends_at": "2030-01-01T00:00:00Z"
        });
        let idempotency = format!("create-{}", Uuid::new_v4());
        let create_request = |body: Value| {
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/control/subscriptions")
                .header(header::COOKIE, &admin)
                .header(header::ORIGIN, "http://127.0.0.1:8080")
                .header("Idempotency-Key", &idempotency)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let response = router
            .clone()
            .oneshot(create_request(subscription_body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let created: Value = serde_json::from_slice(&bytes).unwrap();
        let subscription_id: Uuid = serde_json::from_value(created["id"].clone()).unwrap();

        let response = router
            .clone()
            .oneshot(create_request(subscription_body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let repeated: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(repeated["id"], json!(subscription_id));

        let mut conflicting = subscription_body;
        conflicting["ends_at"] = json!("2031-01-01T00:00:00Z");
        let response = router
            .clone()
            .oneshot(create_request(conflicting))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let conflict: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(conflict["code"], json!("idempotency_conflict"));

        let (status, _, entitlements) = send_method(
            &router,
            Method::GET,
            &format!(
                "/api/v1/control/organizations/{organization}/entitlements?product_id={product}"
            ),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(entitlements["effective"], json!(true));
        assert_eq!(entitlements["features"]["sync"], json!(true));

        let (status, _, _) = send_method(
            &router,
            Method::POST,
            &format!("/api/v1/control/subscriptions/{subscription_id}/suspend"),
            Some(&admin),
            origin,
            Some(json!({"reason": "review"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, suspended) = send_method(
            &router,
            Method::GET,
            &format!(
                "/api/v1/control/organizations/{organization}/entitlements?product_id={product}"
            ),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(suspended["effective"], json!(false));
        assert_eq!(suspended["reason"], json!("suspended"));

        let (status, _, _) = send_method(
            &router,
            Method::POST,
            &format!("/api/v1/control/subscriptions/{subscription_id}/reactivate"),
            Some(&admin),
            origin,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _, new_plan) = send_method(
            &router,
            Method::POST,
            "/api/v1/control/plans",
            Some(&admin),
            origin,
            Some(json!({
                "product_id": product,
                "key": "pro-2",
                "name": "Pro 2",
                "status": "active",
                "limits": {"seats": 20}
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let new_plan_id: Uuid = serde_json::from_value(new_plan["id"].clone()).unwrap();

        let renew_request = Request::builder()
            .method(Method::POST)
            .uri(format!(
                "/api/v1/control/subscriptions/{subscription_id}/renew"
            ))
            .header(header::COOKIE, &admin)
            .header(header::ORIGIN, "http://127.0.0.1:8080")
            .header("Idempotency-Key", format!("renew-{}", Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"plan_id": new_plan_id, "ends_at": "2031-01-01T00:00:00Z"}).to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(renew_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let renewed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            renewed["renewed_from_subscription_id"],
            json!(subscription_id)
        );

        let (status, _, old) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/control/subscriptions/{subscription_id}"),
            Some(&reader),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(old["subscription"]["status"], json!("cancelled"));
        assert!(
            old["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["event"] == json!("cancelled"))
        );

        let (status, _, _) = send_method(
            &router,
            Method::PATCH,
            &format!("/api/v1/control/plans/{plan_id}"),
            Some(&admin),
            origin,
            Some(json!({"features": {"sync": false}})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, _, _) = send_method(
            &router,
            Method::POST,
            "/api/v1/control/plans",
            Some(&reader),
            origin,
            Some(json!({
                "product_id": product,
                "key": "reader-plan",
                "name": "Reader plan"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _, _) = send_method(
            &router,
            Method::POST,
            "/api/v1/control/subscriptions",
            Some(&admin),
            origin,
            Some(json!({
                "organization_id": organization,
                "product_id": product,
                "plan_id": new_plan_id,
                "starts_at": "2026-01-01T00:00:00Z",
                "ends_at": "2030-01-01T00:00:00Z"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn portal_commercial_products_are_read_only_and_tenant_isolated() {
        let pool = open(&isolated_test_url()).await.unwrap();
        let email = format!("portal-commercial-{}@example.test", Uuid::new_v4());
        let password = "contraseña-api-larga";
        seed_credential(&pool, &email, password).await;
        let account: Uuid =
            sqlx::query_scalar("SELECT id FROM commercial_account WHERE email = $1")
                .bind(&email)
                .fetch_one(&pool)
                .await
                .unwrap();
        let organization = Uuid::new_v4();
        let other_organization = Uuid::new_v4();
        let product = Uuid::new_v4();
        let plan = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Portal commercial org', 'active', now()),
                    ($2, 'Other org', 'active', now())",
        )
        .bind(organization)
        .bind(other_organization)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, 'Portal product', '', 'active', 'saas', now(), now())",
        )
        .bind(product)
        .bind(format!("portal-{}", Uuid::new_v4().simple()))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_product (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, 'active', now())",
        )
        .bind(organization)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, 'owner', 1, now())",
        )
        .bind(organization)
        .bind(account)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO plan (id, product_id, key, name, description, status, is_trial, created_at, updated_at)
             VALUES ($1, $2, 'portal-plan', 'Portal plan', '', 'active', false, now(), now())",
        )
        .bind(plan)
        .bind(product)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO subscription
             (id, organization_id, product_id, plan_id, status, is_trial, starts_at, ends_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 'active', false, now() - interval '1 day', now() + interval '30 days', now(), now())",
        )
        .bind(Uuid::new_v4())
        .bind(organization)
        .bind(product)
        .bind(plan)
        .execute(&pool)
        .await
        .unwrap();

        let router = build_router(state(pool.clone()), false, test_auth_state(pool.clone()));
        let (status, headers, _) = send_auth(
            &router,
            "/api/v1/auth/login",
            None,
            Some("http://127.0.0.1:8080"),
            Some(json!({"email": email, "password": password})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let cookie = session_from(&headers);

        let (status, _, products) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{organization}/products"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(products["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            products["items"][0]["entitlements"]["effective"],
            json!(true)
        );
        assert_eq!(
            products["items"][0]["subscription"]["plan"]["key"],
            json!("portal-plan")
        );

        let (status, _, product_detail) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{organization}/products/{product}"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(product_detail["product"]["id"], json!(product));

        let (status, _, _) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{other_organization}/products"),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _, _) = send_method(
            &router,
            Method::GET,
            &format!("/api/v1/organizations/{organization}/products"),
            None,
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
