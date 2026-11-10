use base64::Engine as _;
use ed25519_dalek::Signer;
use platform_application::enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, EnrollmentService, ProofAttempt,
};
use platform_core::{
    enrollment::canonical_proof_message,
    ids::{CommercialAccountId, InstallationId, OrganizationId},
};
use platform_storage_postgres::{PgEnrollmentRepository, open};
use rand_core::OsRng;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

const TEST_PRODUCT_KEY: &str = "enrollment-fixture-product";

mod concurrency;
mod fixtures;
mod idempotency;
mod lifecycle;
