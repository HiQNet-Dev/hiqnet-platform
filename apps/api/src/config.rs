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
        let dev_issuance_setting = std::env::var("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE").ok();
        let requested_dev_issuance = dev_issuance_setting.as_deref() == Some("true");
        let allow_license_issuance = std::env::var("HIQNET_PLATFORM_ALLOW_LICENSE_ISSUANCE")
            .map(|value| value == "true")
            .unwrap_or(false);
        let admin_token_setting = std::env::var("HIQNET_PLATFORM_ADMIN_TOKEN").ok();
        let admin_token_configured = admin_token_setting.is_some();
        let requested_admin_token = admin_token_setting.filter(|value| !value.is_empty());

        let development = std::env::var("HIQNET_PLATFORM_ENV")
            .map(|value| value == "development")
            .unwrap_or(false);
        // Variables DEV heredadas no deben habilitar rutas legacy ni emisión
        // fuera de development, pero tampoco impedir el arranque del servicio.
        if !development {
            if dev_issuance_setting.is_some() {
                tracing::warn!("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE se ignora fuera de development");
            }
            if admin_token_configured {
                tracing::warn!("HIQNET_PLATFORM_ADMIN_TOKEN se ignora fuera de development");
            }
        }
        let (allow_dev_issuance, admin_token) =
            dev_only_access(development, requested_dev_issuance, requested_admin_token);
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

fn dev_only_access(
    development: bool,
    requested_issuance: bool,
    requested_token: Option<String>,
) -> (bool, Option<String>) {
    if development {
        (requested_issuance, requested_token)
    } else {
        (false, None)
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

#[cfg(test)]
mod tests {
    use super::{Settings, dev_only_access};

    #[test]
    fn from_env_limits_dev_access_without_database_connection() {
        if std::env::var("HIQNET_CONFIG_TEST_CHILD").as_deref() == Ok("1") {
            let settings = Settings::from_env().expect("configuración sintética válida");
            if settings.development {
                assert!(settings.allow_dev_issuance);
                assert_eq!(settings.admin_token.as_deref(), Some("token-sintetico"));
            } else {
                assert!(!settings.allow_dev_issuance);
                assert!(settings.admin_token.is_none());
            }
            return;
        }

        for environment in [None, Some("production"), Some("development")] {
            let mut command = std::process::Command::new(
                std::env::current_exe().expect("binario de pruebas disponible"),
            );
            command
                .arg("--exact")
                .arg("config::tests::from_env_limits_dev_access_without_database_connection")
                .env_clear()
                .env("HIQNET_CONFIG_TEST_CHILD", "1")
                .env("DATABASE_URL", "postgres://sintetico@127.0.0.1:1/aislado")
                .env("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE", "true")
                .env("HIQNET_PLATFORM_ADMIN_TOKEN", "token-sintetico");
            if let Some(environment) = environment {
                command.env("HIQNET_PLATFORM_ENV", environment);
            }
            let result = command.output().expect("subproceso de pruebas disponible");
            assert!(
                result.status.success(),
                "configuración rechazada: {environment:?}"
            );
        }
    }

    #[test]
    fn dev_admin_token_and_issuance_are_disabled_outside_development() {
        for requested_issuance in [false, true] {
            for requested_token in [None, Some("token-sintetico".into())] {
                let (allow_dev_issuance, admin_token) =
                    dev_only_access(false, requested_issuance, requested_token);
                assert!(!allow_dev_issuance);
                assert!(admin_token.is_none());
            }
        }
    }

    #[test]
    fn dev_admin_token_and_issuance_require_explicit_opt_in_in_development() {
        let (allow_dev_issuance, admin_token) =
            dev_only_access(true, true, Some("token-sintetico".into()));
        assert!(allow_dev_issuance);
        assert_eq!(admin_token.as_deref(), Some("token-sintetico"));

        let (allow_dev_issuance, admin_token) = dev_only_access(true, false, None);
        assert!(!allow_dev_issuance);
        assert!(admin_token.is_none());
    }
}
