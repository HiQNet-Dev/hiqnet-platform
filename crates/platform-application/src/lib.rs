//! Casos de uso y puertos de HiQNet Platform.
//!
//! Sin cobro real ni sesiones comerciales: la identidad P02.1 sólo se invoca
//! desde adaptadores internos confiables hasta incorporar autenticación P02.2.

pub mod audit_security;
pub mod auth;
pub mod commercial;
pub mod commercial_domain;
pub mod control;
pub mod enrollment;
pub mod error;
pub mod installation_control;
pub mod license_issuance;
pub mod organization_product;
pub mod ports;
pub mod product;
pub mod security;
pub mod service;
pub mod staff;

pub use audit_security::{
    AuditEventDetail, AuditEventSummary, AuditFilters, AuditResult, AuditSecurityActor,
    AuditSecurityError, AuditSecurityRepository, AuditSecurityResult, AuditSecurityService,
    CursorKey, Page as AuditSecurityPage, SecurityEventDetail, SecurityEventSummary,
    SecurityFilters, SecurityKind, StaffAuthAction, StaffAuthEventDetail, StaffAuthEventSummary,
    StaffAuthFilters,
};
pub use auth::{AuthRepository, AuthService};
pub use commercial_domain::{
    CommercialActor, CommercialError, CommercialRepository, CommercialResult, CommercialService,
    CreateSubscription, CreateSubscriptionCommand, CursorKey as CommercialCursor, Entitlements,
    NewPlan, Page as CommercialPage, Plan, PlanFilters, PlanPatch, PlanRef, PlanStatus,
    PortalProduct, PortalProductRef, PortalSubscription, RenewSubscription,
    RenewSubscriptionCommand, Subscription, SubscriptionAction, SubscriptionDetail,
    SubscriptionEvent, SubscriptionFilters, SubscriptionStatus, SubscriptionTransition,
};
pub use enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, BeginStorageOutcome, EnrollmentChallengeView,
    EnrollmentCompletion, EnrollmentError, EnrollmentPage, EnrollmentRepository, EnrollmentResult,
    EnrollmentService, InstallationSummary, NewEnrollmentRequest, ProofAttempt, SecurityEvent,
    SecurityEventKind,
};
pub use error::{AppError, AppResult};
pub use organization_product::{
    OrganizationProductActor, OrganizationProductError, OrganizationProductMutation,
    OrganizationProductRepository, OrganizationProductResult, OrganizationProductService,
};
pub use ports::{
    InstallationView, IssueLicenseRequest, LicenseSigner, PlanView, PlatformRepository,
    RegisterInstallationRequest, SharedRepository, SharedSigner,
};
pub use product::{Page, ProductRepository, ProductService, StaffActor};
pub use service::{IssuedLicenseView, PlatformService};
pub use staff::{StaffAuthService, StaffRepository};
