//! Autenticación y sesión comerciales (clientes), separadas de las staff.

use crate::*;

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct LoginRequest {
    #[schema(example = "cuenta@example.test")]
    pub email: String,
    #[schema(example = "contraseña-sintética")]
    pub password: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct MeResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub email: String,
}

pub(crate) fn auth_error(error: platform_application::auth::AuthError) -> HttpError {
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

pub(crate) fn limiter_key(email: &str) -> String {
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
pub(crate) async fn login(
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
pub(crate) async fn logout(
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
pub(crate) async fn me(
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
pub(crate) struct ChangePasswordRequest {
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
pub(crate) async fn change_password(
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
pub(crate) async fn revoke_sessions(
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
