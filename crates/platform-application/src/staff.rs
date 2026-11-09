//! Casos de uso de autenticación del personal interno HiQNet.
//!
//! El actor staff siempre se deriva de una sesión verificada; nunca de un
//! identificador enviado por el cliente. Reutiliza la infraestructura técnica de
//! ADR-0002 (Argon2id, tokens opacos) con entidades y persistencia separadas de
//! la identidad comercial.

use async_trait::async_trait;
use platform_core::staff::{
    AuthenticatedStaffSession, IssuedStaffSession, StaffCredential, StaffIdentity,
};
pub use platform_core::staff::{StaffError, StaffResult};
use time::{Duration, OffsetDateTime};

use crate::security::{generate_session_token, hash_token, verify_dummy, verify_password};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffLoginOutcome {
    Rotated { expires_at: OffsetDateTime },
    CredentialChanged,
    AccountUnavailable,
    Locked,
}

#[async_trait]
pub trait StaffRepository: Send + Sync {
    async fn find_identity_by_email(&self, email: &str) -> StaffResult<Option<StaffIdentity>>;
    async fn get_identity(
        &self,
        account_id: platform_core::ids::StaffAccountId,
    ) -> StaffResult<Option<StaffIdentity>>;
    async fn get_credential(
        &self,
        account_id: platform_core::ids::StaffAccountId,
    ) -> StaffResult<Option<StaffCredential>>;
    async fn create_credential(&self, credential: &StaffCredential) -> StaffResult<()>;
    async fn is_locked(
        &self,
        account_id: platform_core::ids::StaffAccountId,
        now: OffsetDateTime,
    ) -> StaffResult<bool>;
    async fn register_failure(
        &self,
        account_id: platform_core::ids::StaffAccountId,
        now: OffsetDateTime,
    ) -> StaffResult<bool>;
    async fn rotate_session(
        &self,
        expected_hash: &str,
        session: &AuthenticatedStaffSession,
        token_hash: &[u8],
    ) -> StaffResult<StaffLoginOutcome>;
    async fn get_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> StaffResult<Option<AuthenticatedStaffSession>>;
    async fn revoke_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> StaffResult<Option<platform_core::ids::StaffAccountId>>;
    async fn record_event(
        &self,
        account_id: Option<platform_core::ids::StaffAccountId>,
        action: &str,
        occurred_at: OffsetDateTime,
        metadata: serde_json::Value,
    ) -> StaffResult<()>;
}

pub struct StaffAuthService<R: StaffRepository> {
    repository: R,
}

impl<R: StaffRepository> StaffAuthService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Autentica por correo y contraseña devolviendo un token opaco una sola vez.
    /// Cuenta inexistente, deshabilitada, bloqueada o contraseña incorrecta
    /// responden siempre `InvalidCredentials` para evitar enumeración.
    pub async fn login(
        &self,
        email: &str,
        password: &str,
        now: OffsetDateTime,
        ttl: Duration,
    ) -> StaffResult<IssuedStaffSession> {
        if ttl <= Duration::ZERO {
            return Err(StaffError::Storage);
        }
        let Some(identity) = self.repository.find_identity_by_email(email).await? else {
            verify_dummy(password.to_owned()).await;
            self.repository
                .record_event(
                    None,
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "unknown"}),
                )
                .await?;
            return Err(StaffError::InvalidCredentials);
        };

        let Some(credential) = self.repository.get_credential(identity.account_id).await? else {
            verify_dummy(password.to_owned()).await;
            self.repository
                .record_event(
                    Some(identity.account_id),
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "no_credential"}),
                )
                .await?;
            return Err(StaffError::InvalidCredentials);
        };

        if self.repository.is_locked(identity.account_id, now).await? {
            self.repository
                .record_event(
                    Some(identity.account_id),
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "locked"}),
                )
                .await?;
            return Err(StaffError::InvalidCredentials);
        }

        if !verify_password(password.to_owned(), credential.password_hash.clone()).await {
            let locked = self
                .repository
                .register_failure(identity.account_id, now)
                .await?;
            let reason = if locked { "locked" } else { "bad_password" };
            self.repository
                .record_event(
                    Some(identity.account_id),
                    "login_failure",
                    now,
                    serde_json::json!({"reason": reason}),
                )
                .await?;
            return Err(StaffError::InvalidCredentials);
        }

        let (token, token_hash) = generate_session_token();
        let session = AuthenticatedStaffSession {
            session_id: uuid::Uuid::new_v4(),
            account_id: identity.account_id,
            created_at: now,
            expires_at: now + ttl,
        };
        match self
            .repository
            .rotate_session(&credential.password_hash, &session, &token_hash)
            .await?
        {
            StaffLoginOutcome::Rotated { expires_at } => {
                Ok(IssuedStaffSession { token, expires_at })
            }
            outcome => {
                let reason = match outcome {
                    StaffLoginOutcome::CredentialChanged => "credential_changed",
                    StaffLoginOutcome::AccountUnavailable => "account_unavailable",
                    StaffLoginOutcome::Locked => "locked",
                    StaffLoginOutcome::Rotated { .. } => "unexpected",
                };
                self.repository
                    .record_event(
                        Some(identity.account_id),
                        "login_failure",
                        now,
                        serde_json::json!({"reason": reason}),
                    )
                    .await?;
                Err(StaffError::InvalidCredentials)
            }
        }
    }

    pub async fn logout(&self, token: &str, now: OffsetDateTime) -> StaffResult<()> {
        if let Some(account_id) = self
            .repository
            .revoke_session(&hash_token(token), now)
            .await?
        {
            self.repository
                .record_event(Some(account_id), "logout", now, serde_json::json!({}))
                .await?;
        }
        Ok(())
    }

    pub async fn authenticate(
        &self,
        token: &str,
        now: OffsetDateTime,
    ) -> StaffResult<AuthenticatedStaffSession> {
        self.repository
            .get_session(&hash_token(token), now)
            .await?
            .ok_or(StaffError::InvalidSession)
    }

    pub async fn identity(
        &self,
        account_id: platform_core::ids::StaffAccountId,
    ) -> StaffResult<StaffIdentity> {
        self.repository
            .get_identity(account_id)
            .await?
            .ok_or(StaffError::InvalidSession)
    }
}
