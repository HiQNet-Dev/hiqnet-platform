//! Composition root: construye los servicios concretos y el estado de la API.
//!
//! Es el único lugar donde se eligen implementaciones (PostgreSQL, Ed25519,
//! limitadores en memoria). Los módulos de transporte reciben el estado ya
//! ensamblado y no conocen los constructores de infraestructura.

use crate::*;

/// Ensambla el estado de la aplicación y el estado de autenticación.
pub(crate) fn assemble(settings: &Settings, pool: PgPool) -> anyhow::Result<(AppState, AuthState)> {
    let signer = settings
        .signing_key_b64
        .as_deref()
        .map(platform_licensing::Ed25519Signer::from_seed_base64)
        .transpose()
        .map_err(|error| anyhow::anyhow!(error))?
        .map(|signer| Arc::new(signer) as SharedSigner);
    let licensing_enabled = signer.is_some() && settings.allow_dev_issuance;
    let license_issuance_enabled = signer.is_some() && settings.allow_license_issuance;

    let repository: SharedRepository = Arc::new(pool_repository(pool.clone()));
    let service = Arc::new(PlatformService::new(repository, signer));
    let commercial = Arc::new(CommercialService::new(PgCommercialRepository::new(
        pool.clone(),
    )));
    let enrollment = Arc::new(EnrollmentService::new(
        PgEnrollmentRepository::new(pool.clone()),
        settings.enrollment_ttl_seconds,
        settings.enrollment_verification_uri.clone(),
    ));

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
            settings.enrollment_begin_rate_limit,
            std::time::Duration::from_secs(settings.enrollment_rate_window_seconds),
        )),
        enrollment_proof_limiter: Arc::new(auth::LoginLimiter::new(
            settings.enrollment_proof_rate_limit,
            std::time::Duration::from_secs(settings.enrollment_rate_window_seconds),
        )),
        enrollment_approve_limiter: Arc::new(auth::LoginLimiter::new(
            settings.enrollment_approve_rate_limit,
            std::time::Duration::from_secs(settings.enrollment_rate_window_seconds),
        )),
        admin_token: settings.admin_token.clone(),
        licensing_enabled,
        license_issuance_enabled,
        db_pool: pool.clone(),
    };

    let auth_state = AuthState {
        service: Arc::new(platform_application::AuthService::new(
            platform_storage_postgres::PgAuthRepository::new(pool.clone()),
        )),
        cookie_secure: settings.cookie_secure,
        allowed_origins: settings.allowed_origins.clone(),
        development: settings.development,
        session_ttl: time::Duration::hours(settings.session_hours),
        limiter: Arc::new(auth::LoginLimiter::new(
            10,
            std::time::Duration::from_secs(60),
        )),
        argon2_slots: Arc::new(tokio::sync::Semaphore::new(settings.auth_concurrency)),
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
            session_ttl: time::Duration::hours(settings.staff_session_hours),
            argon2_slots: Arc::new(tokio::sync::Semaphore::new(settings.auth_concurrency)),
        },
    };

    Ok((state, auth_state))
}
