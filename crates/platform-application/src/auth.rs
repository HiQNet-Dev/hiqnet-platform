//! Casos de uso de autenticación comercial.
//!
//! El actor siempre se deriva de una sesión verificada; nunca de identificadores
//! enviados por el cliente.

use async_trait::async_trait;
pub use platform_core::auth::AuthError;
use platform_core::{
    auth::{
        AuthResult, AuthenticatedSession, CommercialCredential, CommercialIdentity, IssuedSession,
        validate_password,
    },
    ids::CommercialAccountId,
};
use time::{Duration, OffsetDateTime};

use crate::security::{
    generate_session_token, hash_password, hash_token, verify_dummy, verify_password,
};

#[async_trait]
pub trait AuthRepository: Send + Sync {
    async fn find_identity_by_email(&self, email: &str) -> AuthResult<Option<CommercialIdentity>>;
    async fn get_identity(
        &self,
        account_id: CommercialAccountId,
    ) -> AuthResult<Option<CommercialIdentity>>;
    async fn get_credential(
        &self,
        account_id: CommercialAccountId,
    ) -> AuthResult<Option<CommercialCredential>>;
    async fn create_credential(&self, credential: &CommercialCredential) -> AuthResult<()>;
    async fn is_locked(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<bool>;
    async fn register_failure(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<bool>;
    async fn clear_failures(&self, account_id: CommercialAccountId) -> AuthResult<()>;
    async fn create_session(
        &self,
        session: &AuthenticatedSession,
        token_hash: &[u8],
    ) -> AuthResult<()>;
    async fn get_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<Option<AuthenticatedSession>>;
    async fn revoke_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<Option<CommercialAccountId>>;
    async fn revoke_account_sessions(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<()>;
    async fn record_event(
        &self,
        account_id: Option<CommercialAccountId>,
        action: &str,
        occurred_at: OffsetDateTime,
        metadata: serde_json::Value,
    ) -> AuthResult<()>;

    /// Cambia el hash sólo si sigue siendo `expected_hash`, revoca todas las
    /// sesiones y crea `session` en la misma transacción. Devuelve `false` si
    /// otra operación modificó la credencial.
    async fn change_password(
        &self,
        account_id: CommercialAccountId,
        expected_hash: &str,
        new_hash: &str,
        session: &AuthenticatedSession,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<bool>;

    /// Revoca todas las sesiones activas y audita el evento en una transacción.
    async fn revoke_all_sessions(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<()>;
}

pub struct AuthService<R: AuthRepository> {
    repository: R,
}

impl<R: AuthRepository> AuthService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Autentica por correo y contraseña. Devuelve un token opaco sólo una vez.
    ///
    /// Ante cuenta inexistente, deshabilitada, bloqueada o contraseña incorrecta
    /// responde siempre `InvalidCredentials` para evitar enumeración.
    pub async fn login(
        &self,
        email: &str,
        password: &str,
        now: OffsetDateTime,
        ttl: Duration,
    ) -> AuthResult<IssuedSession> {
        if ttl <= Duration::ZERO {
            return Err(AuthError::Storage);
        }
        let identity = self.repository.find_identity_by_email(email).await?;
        let Some(identity) = identity else {
            verify_dummy(password.to_owned()).await;
            self.repository
                .record_event(
                    None,
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "unknown"}),
                )
                .await?;
            return Err(AuthError::InvalidCredentials);
        };

        let credential = self.repository.get_credential(identity.account_id).await?;
        let Some(credential) = credential else {
            verify_dummy(password.to_owned()).await;
            self.repository
                .record_event(
                    Some(identity.account_id),
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "no_credential"}),
                )
                .await?;
            return Err(AuthError::InvalidCredentials);
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
            return Err(AuthError::InvalidCredentials);
        }

        if !verify_password(password.to_owned(), credential.password_hash).await {
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
            return Err(AuthError::InvalidCredentials);
        }

        self.repository.clear_failures(identity.account_id).await?;
        // Rotación: cualquier sesión previa deja de ser válida al iniciar sesión.
        self.repository
            .revoke_account_sessions(identity.account_id, now)
            .await?;

        let (token, token_hash) = generate_session_token();
        let session = AuthenticatedSession {
            session_id: uuid::Uuid::new_v4(),
            account_id: identity.account_id,
            created_at: now,
            expires_at: now + ttl,
        };
        self.repository
            .create_session(&session, &token_hash)
            .await?;
        self.repository
            .record_event(
                Some(identity.account_id),
                "login_success",
                now,
                serde_json::json!({}),
            )
            .await?;
        Ok(IssuedSession {
            token,
            expires_at: session.expires_at,
        })
    }

    /// Revoca la sesión del token presentado. Idempotente y sin filtrar existencia.
    pub async fn logout(&self, token: &str, now: OffsetDateTime) -> AuthResult<()> {
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
    ) -> AuthResult<AuthenticatedSession> {
        self.repository
            .get_session(&hash_token(token), now)
            .await?
            .ok_or(AuthError::InvalidSession)
    }

    pub async fn identity(
        &self,
        account_id: CommercialAccountId,
    ) -> AuthResult<CommercialIdentity> {
        self.repository
            .get_identity(account_id)
            .await?
            .ok_or(AuthError::InvalidSession)
    }

    /// Cambia la contraseña verificando la actual y sustituyendo todas las
    /// sesiones por una nueva. Un cambio concurrente produce `Conflict` sin
    /// dejar escrituras parciales.
    pub async fn change_password(
        &self,
        account_id: CommercialAccountId,
        current_password: &str,
        new_password: &str,
        now: OffsetDateTime,
        ttl: Duration,
    ) -> AuthResult<IssuedSession> {
        if ttl <= Duration::ZERO {
            return Err(AuthError::Storage);
        }
        self.repository
            .get_identity(account_id)
            .await?
            .ok_or(AuthError::InvalidSession)?;
        if self.repository.is_locked(account_id, now).await? {
            return Err(AuthError::InvalidCredentials);
        }
        let credential = self
            .repository
            .get_credential(account_id)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        if !verify_password(
            current_password.to_owned(),
            credential.password_hash.clone(),
        )
        .await
        {
            self.repository.register_failure(account_id, now).await?;
            self.repository
                .record_event(
                    Some(account_id),
                    "login_failure",
                    now,
                    serde_json::json!({"reason": "password_change"}),
                )
                .await?;
            return Err(AuthError::InvalidCredentials);
        }
        if current_password == new_password {
            return Err(AuthError::WeakPassword);
        }
        validate_password(new_password)?;

        let new_hash = hash_password(new_password.to_owned()).await?;
        let (token, token_hash) = generate_session_token();
        let session = AuthenticatedSession {
            session_id: uuid::Uuid::new_v4(),
            account_id,
            created_at: now,
            expires_at: now + ttl,
        };
        let changed = self
            .repository
            .change_password(
                account_id,
                &credential.password_hash,
                &new_hash,
                &session,
                &token_hash,
                now,
            )
            .await?;
        if !changed {
            return Err(AuthError::Conflict);
        }
        Ok(IssuedSession {
            token,
            expires_at: session.expires_at,
        })
    }

    /// Revoca todas las sesiones de la cuenta autenticada, incluida la actual.
    pub async fn revoke_all_sessions(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<()> {
        self.repository.revoke_all_sessions(account_id, now).await
    }
}
