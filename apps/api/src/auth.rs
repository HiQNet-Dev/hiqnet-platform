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
    /// Sólo true con `HIQNET_PLATFORM_ENV=development`: permite curl/Swagger sin Origin.
    pub development: bool,
    pub session_ttl: time::Duration,
    pub limiter: Arc<LoginLimiter>,
    /// Cota de verificaciones Argon2id simultáneas para proteger CPU.
    pub argon2_slots: Arc<tokio::sync::Semaphore>,
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

/// Política CSRF para mutaciones de autenticación:
/// - en producción el encabezado `Origin` es obligatorio y debe coincidir exacto;
/// - `null`, valores múltiples o malformados se rechazan siempre;
/// - sólo `development` admite ausencia de Origin, para curl/Swagger locales.
pub fn origin_allowed(headers: &HeaderMap, state: &AuthState) -> bool {
    let Some(allowed) = state.allowed_origin.as_deref() else {
        return false;
    };
    let mut values = headers.get_all(header::ORIGIN).iter();
    let Some(value) = values.next() else {
        return state.development;
    };
    if values.next().is_some() {
        return false;
    }
    let Ok(origin) = value.to_str() else {
        return false;
    };
    origin != "null" && origin == allowed
}

/// Ventana deslizante en memoria. Suficiente para una instancia; un despliegue
/// multi-instancia necesitará un limitador compartido.
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    limit: usize,
    window: Duration,
}

/// Cota de claves en memoria por instancia.
pub const MAX_LIMITER_KEYS: usize = 4096;

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

    fn prune_map(attempts: &mut HashMap<String, Vec<Instant>>, window: Duration) {
        attempts.retain(|_, entries| {
            Self::prune(entries, window);
            !entries.is_empty()
        });
    }

    /// Si la capacidad está llena de claves activas, una clave desconocida se
    /// rechaza (fail closed) en lugar de vaciar el mapa y olvidar bloqueos.
    pub fn is_limited(&self, key: &str) -> bool {
        let Ok(mut attempts) = self.attempts.lock() else {
            return true;
        };
        if let Some(entries) = attempts.get_mut(key) {
            Self::prune(entries, self.window);
            return entries.len() >= self.limit;
        }
        if attempts.len() >= MAX_LIMITER_KEYS {
            Self::prune_map(&mut attempts, self.window);
        }
        attempts.len() >= MAX_LIMITER_KEYS
    }

    pub fn record(&self, key: &str) {
        if let Ok(mut attempts) = self.attempts.lock() {
            if !attempts.contains_key(key) && attempts.len() >= MAX_LIMITER_KEYS {
                Self::prune_map(&mut attempts, self.window);
                if attempts.len() >= MAX_LIMITER_KEYS {
                    // Desaloja sólo una clave sin bloqueo activo; las claves
                    // limitadas se conservan para no permitir evasión.
                    let victim = attempts
                        .iter()
                        .filter(|(_, entries)| entries.len() < self.limit)
                        .min_by_key(|(_, entries)| entries.last().copied())
                        .map(|(key, _)| key.clone());
                    match victim {
                        Some(victim) => {
                            attempts.remove(&victim);
                        }
                        None => return,
                    }
                }
            }
            let entries = attempts.entry(key.to_owned()).or_default();
            Self::prune(entries, self.window);
            entries.push(Instant::now());
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.attempts.lock().map(|map| map.len()).unwrap_or(0)
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

    #[test]
    fn limiter_acota_memoria_sin_olvidar_claves_bloqueadas() {
        let limiter = LoginLimiter::new(3, Duration::from_secs(60));
        for _ in 0..3 {
            limiter.record("bloqueada");
        }
        assert!(limiter.is_limited("bloqueada"));

        for index in 0..(MAX_LIMITER_KEYS * 2) {
            limiter.record(&format!("clave-{index}"));
        }
        assert!(
            limiter.len() <= MAX_LIMITER_KEYS,
            "el mapa debe permanecer acotado"
        );
        assert!(
            limiter.is_limited("bloqueada"),
            "la clave bloqueada no debe desalojarse"
        );
    }

    #[test]
    fn limiter_recupera_capacidad_al_expirar_la_ventana() {
        // Ventana cero simula expiración sin sleeps: las marcas caducan al instante.
        let limiter = LoginLimiter::new(1, Duration::ZERO);
        for index in 0..MAX_LIMITER_KEYS {
            limiter.record(&format!("clave-{index}"));
        }
        assert!(!limiter.is_limited("clave-nueva"));
        assert_eq!(limiter.len(), 0, "las claves caducadas se depuran");
    }
}
