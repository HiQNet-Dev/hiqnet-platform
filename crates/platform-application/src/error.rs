use platform_core::DomainError;

/// `PartialEq` soporta aserciones directas en las pruebas de dominio.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("no se encontró el recurso solicitado")]
    NotFound,

    #[error("el recurso ya existe y no puede duplicarse")]
    Conflict,

    #[error("no fue posible completar la operación de almacenamiento")]
    Storage,

    #[error("no fue posible emitir la autorización comercial")]
    LicensingDisabled,

    #[error("la administración no está configurada")]
    AdminDisabled,

    #[error("acceso denegado")]
    Unauthorized,

    #[error("la organización no tiene derechos comerciales vigentes para emitir")]
    NotEntitled,

    #[error("el plan contratado no es representable en el contrato de licencia v1")]
    UnsupportedPlan,

    #[error("una capacidad del plan no es representable en el contrato de licencia v1")]
    UnsupportedFeature,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Domain(DomainError::DuplicateInstallation) => "conflict",
            AppError::Domain(DomainError::InstallationNotFound) => "not_found",
            AppError::Domain(DomainError::TrialAlreadyActivated) => "conflict",
            AppError::Domain(DomainError::LicensingDisabled) => "licensing_disabled",
            AppError::Domain(DomainError::CommercialStateBlocked) => "commercial_state_blocked",
            AppError::Domain(_) => "validation_error",
            AppError::NotFound => "not_found",
            AppError::Conflict => "conflict",
            AppError::Storage => "storage_error",
            AppError::LicensingDisabled => "licensing_disabled",
            AppError::AdminDisabled => "admin_disabled",
            AppError::Unauthorized => "unauthorized",
            AppError::NotEntitled => "not_entitled",
            AppError::UnsupportedPlan => "unsupported_plan",
            AppError::UnsupportedFeature => "unsupported_feature",
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
