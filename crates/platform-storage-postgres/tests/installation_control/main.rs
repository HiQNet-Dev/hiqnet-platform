//! C5.1D — control administrativo de instalaciones POS.
//!
//! Cubre la migración 0009, el read model keyset, las mutaciones
//! administrativas, la doble autoridad del rehost y el cutover atómico
//! de identidad.

use base64::Engine as _;
use ed25519_dalek::Signer;
use platform_application::enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, EnrollmentChallengeView, EnrollmentError,
    EnrollmentService, ProofAttempt,
};
use platform_application::installation_control::{
    AdministrativeStatus, ControlActor, IdentityStatus, InstallationControlError,
    InstallationControlService, InstallationFilters, RehostStatus,
};
use platform_core::{
    enrollment::canonical_proof_message,
    ids::{
        CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId, ProductId,
        StaffAccountId,
    },
    staff::StaffRole,
};
use platform_storage_postgres::{PgEnrollmentRepository, PgInstallationControlRepository, open};
use rand_core::OsRng;
use sqlx::{PgPool, Row};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

mod control;
mod enrollment;
mod fixtures;
mod migration;
mod rehost;
mod rehost_cutover;
mod rehost_validation;
