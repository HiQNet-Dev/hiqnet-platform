//! Extractores, cookies, CSRF básico y límite de intentos de autenticación.

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, header, request::Parts},
};
use platform_application::auth::AuthService;
use platform_core::auth::AuthenticatedSession;
use platform_storage_postgres::PgAuthRepository;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::contract::ErrorResponse;

pub const SESSION_COOKIE: &str = "hiqnet_session";

#[derive(Clone)]
pub struct AuthState {
    pub service: Arc<AuthService<PgAuthRepository>>,
    pub cookie_secure: bool,
    pub allowed_origin: Option<String>,
    pub session_ttl: time::Duration,
    pub limiter: Arc<LoginLimiter>,
}

pub struct AuthContext {
    pub session: AuthenticatedSession,
}

impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, axum::Json<ErrorResponse>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<AuthState>()
            .cloned()
            .ok_or_else(|| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "auth_unavailable",
                    "autenticación no configurada",
                )
            })?;
        let token = cookie_token(&parts.headers)
            .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "unauthorized", "sesión requerida"))?;
        let session = state
            .service
            .authenticate(&token, time::OffsetDateTime::now_utc())
            .await
            .map_err(|_| {
                error(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "sesión inválida o expirada",
                )
            })?;
        Ok(Self { session })
    }
}

pub fn error(
    status: StatusCode,
    code: &'static str,
    message: &str,
) -> (StatusCode, axum::Json<ErrorResponse>) {
    (
        status,
        axum::Json(ErrorResponse {
            code,
            message: message.to_owned(),
        }),
    )
}

pub fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == SESSION_COOKIE).then(|| value.to_owned())
            })
        })
}

pub fn session_cookie(state: &AuthState, token: &str, ttl: Duration) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; Path=/api; SameSite=Lax; Max-Age={}",
        ttl.as_secs()
    );
    if state.cookie_secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_cookie(state: &AuthState) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; HttpOnly; Path=/api; SameSite=Lax; Max-Age=0");
    if state.cookie_secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Si el navegador envía Origin, debe coincidir con el origen permitido.
/// Sin Origin se acepta para clientes no navegador (curl, Swagger local) siempre
/// que se haya configurado explícitamente un origen permitido.
pub fn origin_allowed(headers: &HeaderMap, state: &AuthState) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return state.allowed_origin.is_some();
    };
    state
        .allowed_origin
        .as_deref()
        .is_some_and(|allowed| allowed == origin)
}

/// Ventana deslizante en memoria. Suficiente para una instancia; un despliegue
/// multi-instancia necesitará un limitador compartido.
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    limit: usize,
    window: Duration,
}

impl LoginLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            limit,
            window,
        }
    }

    fn prune(entries: &mut Vec<Instant>, window: Duration) {
        let now = Instant::now();
        entries.retain(|instant| now.duration_since(*instant) < window);
    }

    pub fn is_limited(&self, key: &str) -> bool {
        let Ok(mut attempts) = self.attempts.lock() else {
            return true;
        };
        let entries = attempts.entry(key.to_owned()).or_default();
        Self::prune(entries, self.window);
        entries.len() >= self.limit
    }

    pub fn record(&self, key: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            // Cota de memoria: limpia claves caducadas y, si persiste la presión,
            // reinicia el limitador antes que crecer sin límite.
            if attempts.len() >= 4096 {
                attempts.retain(|_, entries| {
                    Self::prune(entries, self.window);
                    !entries.is_empty()
                });
                if attempts.len() >= 4096 {
                    attempts.clear();
                }
            }
            let entries = attempts.entry(key.to_owned()).or_default();
            Self::prune(entries, self.window);
            entries.push(Instant::now());
        }
    }

    pub fn reset(&self, key: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            attempts.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_bloquea_al_alcanzar_el_umbral_y_se_libera() {
        let limiter = LoginLimiter::new(2, Duration::from_secs(60));
        assert!(!limiter.is_limited("k"));
        limiter.record("k");
        limiter.record("k");
        assert!(limiter.is_limited("k"));
        limiter.reset("k");
        assert!(!limiter.is_limited("k"));
    }
}
