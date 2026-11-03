#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("el nombre es obligatorio")]
    EmptyName,

    #[error("el plan comercial no es válido")]
    InvalidPlan,

    #[error("la instalación ya existe")]
    DuplicateInstallation,

    #[error("la instalación no existe")]
    InstallationNotFound,

    #[error("el Trial ya fue activado para esta instalación")]
    TrialAlreadyActivated,

    #[error("el periodo de vigencia no es válido")]
    InvalidValidity,

    #[error("no fue posible emitir la autorización comercial")]
    LicensingDisabled,

    #[error("el estado comercial no permite esta operación")]
    CommercialStateBlocked,
}

pub type DomainResult<T> = Result<T, DomainError>;
