//! Persistencia del enrolamiento comercial. No conoce Axum ni DTOs HTTP.
//!
//! Organizado por responsabilidad: errores, cursor, helpers y repositorio.
//! Las transacciones y el protocolo Ed25519 viven en `repository.rs`.

//! Persistencia del enrolamiento comercial. No conoce Axum ni DTOs HTTP.

use async_trait::async_trait;
use base64::Engine as _;
use platform_application::enrollment::{
    ApprovedEnrollment, BeginStorageOutcome, EnrollmentCompletion, EnrollmentError, EnrollmentPage,
    EnrollmentProduct, EnrollmentRepository, EnrollmentResult, InstallationAttribution,
    InstallationSummary, NewEnrollmentRequest, ProofAttempt, SecurityEvent,
};
use platform_core::{
    enrollment::{self, DevicePublicKey, canonical_proof_message, verify_device_signature},
    ids::{CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId, ProductId},
    product::LifecycleStatus,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

mod cursor;
mod errors;
mod helpers;
mod proof;
mod repository;

use cursor::*;
use errors::*;
use helpers::*;

pub struct PgEnrollmentRepository {
    pool: PgPool,
}

impl PgEnrollmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
