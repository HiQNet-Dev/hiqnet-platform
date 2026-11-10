//! Autenticación del personal interno: cookie, extractor y handlers HTTP.
//!
//! Separado de la autenticación comercial: cookie distinta, rutas bajo
//! `/api/v1/control` y security scheme `staffSession`.

use axum::{
    Json,
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::IntoResponse,
};
use platform_application::control::ControlService;
use platform_application::installation_control::InstallationControlService;
use platform_application::product::ProductService;
use platform_application::staff::StaffAuthService;
use platform_core::staff::{AuthenticatedStaffSession, StaffError};
use platform_storage_postgres::{
    PgControlRepository, PgInstallationControlRepository, PgProductRepository, PgStaffRepository,
};
use std::sync::Arc;

use crate::auth::{self, AuthState, LoginLimiter};
use crate::contract::ErrorResponse;

pub const STAFF_SESSION_COOKIE: &str = "hiqnet_staff_session";

#[derive(Clone)]
pub struct StaffState {
    pub service: Arc<StaffAuthService<PgStaffRepository>>,
    pub product: Arc<ProductService<PgProductRepository>>,
    pub control: Arc<ControlService<PgControlRepository>>,
    pub installations: Arc<InstallationControlService<PgInstallationControlRepository>>,
    pub limiter: Arc<LoginLimiter>,
    pub session_ttl: time::Duration,
    pub argon2_slots: Arc<tokio::sync::Semaphore>,
}

pub struct StaffContext {
    pub session: AuthenticatedStaffSession,
}

impl<S> FromRequestParts<S> for StaffContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<AuthState>()
            .cloned()
            .ok_or_else(|| {
                auth::error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "staff_auth_unavailable",
                    "autenticación staff no configurada",
                )
            })?;
        let token = staff_cookie_token(&parts.headers).ok_or_else(|| {
            auth::error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "sesión staff requerida",
            )
        })?;
        let session = state
            .staff
            .service
            .authenticate(&token, time::OffsetDateTime::now_utc())
            .await
            .map_err(|failure| match failure {
                StaffError::InvalidSession => auth::error(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "sesión staff inválida o expirada",
                ),
                _ => auth::error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage_error",
                    "no fue posible verificar la sesión staff",
                ),
            })?;
        Ok(Self { session })
    }
}

pub fn staff_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == STAFF_SESSION_COOKIE).then(|| value.to_owned())
            })
        })
}

fn session_cookie(state: &AuthState, token: &str, ttl: std::time::Duration) -> String {
    let mut cookie = format!(
        "{STAFF_SESSION_COOKIE}={token}; HttpOnly; Path=/api/v1/control; SameSite=Strict; Max-Age={}",
        ttl.as_secs()
    );
    if state.cookie_secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_cookie(state: &AuthState) -> String {
    let mut cookie = format!(
        "{STAFF_SESSION_COOKIE}=; HttpOnly; Path=/api/v1/control; SameSite=Strict; Max-Age=0"
    );
    if state.cookie_secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub(crate) fn staff_error(error: StaffError) -> (StatusCode, Json<ErrorResponse>) {
    match error {
        StaffError::InvalidCredentials => auth::error(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "credenciales inválidas",
        ),
        StaffError::RateLimited => auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiados intentos; inténtalo más tarde",
        ),
        StaffError::InvalidSession => auth::error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "sesión staff inválida o expirada",
        ),
        StaffError::WeakPassword => auth::error(
            StatusCode::BAD_REQUEST,
            "weak_password",
            "la contraseña no cumple la política",
        ),
        StaffError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "conflict",
            "la operación entró en conflicto",
        ),
        StaffError::Storage => auth::error(
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

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct StaffLoginRequest {
    #[schema(example = "staff@hiqnet.test")]
    pub email: String,
    #[schema(example = "contraseña-staff-sintética")]
    pub password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct StaffMeResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub capabilities: Vec<String>,
}

/// Inicia una sesión staff con credenciales verificadas.
#[utoipa::path(post, path = "/api/v1/control/auth/login", operation_id = "staffLogin", tag = "Control Center",
    request_body(content = StaffLoginRequest, description = "Credenciales de personal interno; la contraseña nunca se registra"),
    responses(
        (status = 204, description = "Sesión staff creada; cookie HttpOnly hiqnet_staff_session"),
        (status = 401, description = "Credenciales inválidas, cuenta deshabilitada o bloqueada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 429, description = "Demasiados intentos", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible o verificación saturada", body = ErrorResponse)))]
pub async fn staff_login(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    headers: HeaderMap,
    Json(request): Json<StaffLoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let key = limiter_key(&request.email);
    if auth_state.staff.limiter.is_limited(&key) {
        return Err(staff_error(StaffError::RateLimited));
    }
    let _permit = auth_state
        .staff
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
        .staff
        .service
        .login(
            &request.email,
            &request.password,
            time::OffsetDateTime::now_utc(),
            auth_state.staff.session_ttl,
        )
        .await
    {
        Ok(issued) => issued,
        Err(error) => {
            if matches!(error, StaffError::InvalidCredentials) {
                auth_state.staff.limiter.record(&key);
            }
            return Err(staff_error(error));
        }
    };
    auth_state.staff.limiter.reset(&key);
    let cookie = session_cookie(
        &auth_state,
        &issued.token,
        std::time::Duration::from_secs(auth_state.staff.session_ttl.whole_seconds().max(0) as u64),
    );
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&cookie).map_err(|_| staff_error(StaffError::Storage))?,
    );
    Ok(response)
}

/// Cierra la sesión staff y borra la cookie. Idempotente.
#[utoipa::path(post, path = "/api/v1/control/auth/logout", operation_id = "staffLogout", tag = "Control Center",
    security(("staffSession" = [])),
    responses(
        (status = 204, description = "Sesión staff revocada y cookie eliminada"),
        (status = 403, description = "Origen no permitido", body = ErrorResponse)))]
pub async fn staff_logout(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    if let Some(token) = staff_cookie_token(&headers) {
        auth_state
            .staff
            .service
            .logout(&token, time::OffsetDateTime::now_utc())
            .await
            .map_err(staff_error)?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&clear_cookie(&auth_state))
            .map_err(|_| staff_error(StaffError::Storage))?,
    );
    Ok(response)
}

/// Identidad staff derivada exclusivamente de la sesión verificada.
#[utoipa::path(get, path = "/api/v1/control/auth/me", operation_id = "staffMe", tag = "Control Center",
    security(("staffSession" = [])),
    responses(
        (status = 200, description = "Identidad staff autenticada con sus capabilities", body = StaffMeResponse),
        (status = 401, description = "Sesión staff inválida, expirada o revocada", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub async fn staff_me(
    axum::Extension(auth_state): axum::Extension<AuthState>,
    staff_context: StaffContext,
) -> Result<Json<StaffMeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let identity = auth_state
        .staff
        .service
        .identity(staff_context.session.account_id)
        .await
        .map_err(staff_error)?;
    let capabilities = identity
        .role
        .capabilities()
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect();
    Ok(Json(StaffMeResponse {
        account_id: identity.account_id.as_uuid(),
        email: identity.email,
        display_name: identity.display_name,
        role: identity.role.as_str().to_owned(),
        capabilities,
    }))
}
