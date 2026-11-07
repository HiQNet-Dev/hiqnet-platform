//! Casos de uso y puertos de HiQNet Platform.
//!
//! Sin cobro real ni sesiones comerciales: la identidad P02.1 sólo se invoca
//! desde adaptadores internos confiables hasta incorporar autenticación P02.2.

pub mod auth;
pub mod commercial;
pub mod error;
pub mod ports;
pub mod security;
pub mod service;

pub use auth::{AuthRepository, AuthService};
pub use error::{AppError, AppResult};
pub use ports::{
    InstallationView, IssueLicenseRequest, LicenseSigner, PlanView, PlatformRepository,
    RegisterInstallationRequest, SharedRepository, SharedSigner,
};
pub use service::{IssuedLicenseView, PlatformService};
