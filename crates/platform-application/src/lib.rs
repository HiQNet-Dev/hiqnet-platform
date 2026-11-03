//! Casos de uso y puertos de HiQNet Platform.
//!
//! La plataforma no implementa cobro real ni SLA; solo registra instalaciones,
//! activa Trials y emite autorizaciones firmadas para POS locales.

pub mod error;
pub mod ports;
pub mod service;

pub use error::{AppError, AppResult};
pub use ports::{
    InstallationView, IssueLicenseRequest, LicenseSigner, PlanView, PlatformRepository,
    RegisterInstallationRequest, SharedRepository, SharedSigner,
};
pub use service::{IssuedLicenseView, PlatformService};
