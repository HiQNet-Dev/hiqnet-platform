//! Configuración de proceso: lectura y validación de variables de entorno.
//!
//! Toda la política de configuración (valores por defecto, límites y reglas de
//! seguridad del arranque) vive aquí. El composition root sólo consume el
//! resultado, de modo que no hay `env::var` disperso por los módulos.

/// Configuración inmutable resuelta una vez al arrancar.
pub(crate) struct Settings {
    pub(crate) database_url: String,
    pub(crate) signing_key_b64: Option<String>,
    pub(crate) allow_dev_issuance: bool,
    /// Habilitación explícita de la emisión comercial controlada. Sin ella el
    /// firmante sólo sirve a los flujos DEV; la emisión productiva sigue
    /// prohibida hasta que exista custodia de clave autorizada.
    pub(crate) allow_license_issuance: bool,
    pub(crate) admin_token: Option<String>,
    pub(crate) development: bool,
    pub(crate) cookie_secure: bool,
    pub(crate) allowed_origins: Vec<String>,
    pub(crate) session_hours: i64,
    pub(crate) staff_session_hours: i64,
    pub(crate) auth_concurrency: usize,
    pub(crate) enrollment_ttl_seconds: u64,
    pub(crate) enrollment_verification_uri: String,
    pub(crate) enrollment_begin_rate_limit: usize,
    pub(crate) enrollment_proof_rate_limit: usize,
    pub(crate) enrollment_approve_rate_limit: usize,
    pub(crate) enrollment_rate_window_seconds: u64,
    pub(crate) docs_enabled: bool,
    pub(crate) port: u16,
}

impl Settings {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")?;

        let signing_key_b64 = std::env::var("HIQNET_PLATFORM_SIGNING_KEY_B64")
            .ok()
            .filter(|value| !value.is_empty());
        let allow_dev_issuance = std::env::var("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE")
            .map(|value| value == "true")
            .unwrap_or(false);
        let allow_license_issuance = std::env::var("HIQNET_PLATFORM_ALLOW_LICENSE_ISSUANCE")
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

        let enrollment_ttl_seconds = env_clamped(
            "HIQNET_PLATFORM_ENROLLMENT_TTL_SECONDS",
            platform_core::enrollment::DEFAULT_CHALLENGE_TTL_SECONDS,
            60,
            3600,
        );
        let enrollment_verification_uri =
            std::env::var("HIQNET_PLATFORM_ENROLLMENT_VERIFICATION_URI")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/device".into());

        Ok(Self {
            database_url,
            signing_key_b64,
            allow_dev_issuance,
            allow_license_issuance,
            admin_token,
            development,
            cookie_secure,
            allowed_origins,
            session_hours: env_clamped("HIQNET_PLATFORM_SESSION_TTL_HOURS", 8_i64, 1, 168),
            staff_session_hours: env_clamped(
                "HIQNET_PLATFORM_STAFF_SESSION_TTL_HOURS",
                8_i64,
                1,
                24,
            ),
            auth_concurrency: env_clamped("HIQNET_PLATFORM_AUTH_CONCURRENCY", 4_usize, 1, 32),
            enrollment_ttl_seconds,
            enrollment_verification_uri,
            enrollment_begin_rate_limit: env_clamped(
                "HIQNET_PLATFORM_ENROLLMENT_BEGIN_RATE_LIMIT",
                30_usize,
                1,
                1000,
            ),
            enrollment_proof_rate_limit: env_clamped(
                "HIQNET_PLATFORM_ENROLLMENT_PROOF_RATE_LIMIT",
                60_usize,
                1,
                1000,
            ),
            enrollment_approve_rate_limit: env_clamped(
                "HIQNET_PLATFORM_ENROLLMENT_APPROVE_RATE_LIMIT",
                30_usize,
                1,
                1000,
            ),
            enrollment_rate_window_seconds: env_clamped(
                "HIQNET_PLATFORM_ENROLLMENT_RATE_WINDOW_SECONDS",
                60_u64,
                1,
                3600,
            ),
            // La documentación sólo se expone cuando el entorno la habilita explícitamente.
            docs_enabled: std::env::var("HIQNET_PLATFORM_ENABLE_DOCS")
                .is_ok_and(|value| value == "true"),
            port: env_clamped("PORT", 8080_u16, 1, u16::MAX),
        })
    }
}

fn env_clamped<T>(key: &str, default: T, min: T, max: T) -> T
where
    T: std::str::FromStr + Ord + Copy,
{
    let value = std::env::var(key)
        .ok()
        .and_then(|raw| raw.parse::<T>().ok())
        .unwrap_or(default);
    value.max(min).min(max)
}
