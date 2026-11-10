use platform_application::{
    auth::{AuthRepository as _, AuthService, LoginOutcome},
    commercial::CommercialService,
    security::{generate_session_token, hash_password, hash_token},
};
use platform_core::{
    auth::{AuthenticatedSession, CommercialCredential},
    commercial::CommercialAccount,
};
use platform_storage_postgres::{PgAuthRepository, PgCommercialRepository, open};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

mod credentials;
mod fixtures;
mod sessions;
