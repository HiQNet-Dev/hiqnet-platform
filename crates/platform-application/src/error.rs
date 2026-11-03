use platform_core::DomainError;

#[derive(Debug, thiserror::Error)]
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
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
