//! Persistencia de credenciales, sesiones y eventos de autenticación.

use async_trait::async_trait;
use platform_application::auth::AuthRepository;
use platform_core::{
    auth::{AuthError, AuthResult, AuthenticatedSession, CommercialCredential, CommercialIdentity},
    ids::CommercialAccountId,
};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgAuthRepository {
    pool: PgPool,
}

impl PgAuthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> AuthError {
    AuthError::Storage
}

#[async_trait]
impl AuthRepository for PgAuthRepository {
    async fn find_identity_by_email(&self, email: &str) -> AuthResult<Option<CommercialIdentity>> {
        let email = email.trim().to_lowercase();
        let row = sqlx::query(
            "SELECT id, email FROM commercial_account
             WHERE email = $1 AND status = 'active'",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| CommercialIdentity {
            account_id: CommercialAccountId::from_uuid(row.get("id")),
            email: row.get("email"),
        }))
    }

    async fn get_identity(
        &self,
        account_id: CommercialAccountId,
    ) -> AuthResult<Option<CommercialIdentity>> {
        let row = sqlx::query(
            "SELECT id, email FROM commercial_account
             WHERE id = $1 AND status = 'active'",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| CommercialIdentity {
            account_id: CommercialAccountId::from_uuid(row.get("id")),
            email: row.get("email"),
        }))
    }

    async fn get_credential(
        &self,
        account_id: CommercialAccountId,
    ) -> AuthResult<Option<CommercialCredential>> {
        let row = sqlx::query(
            "SELECT account_id, password_hash, password_changed_at
             FROM commercial_credential WHERE account_id = $1",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| CommercialCredential {
            account_id: CommercialAccountId::from_uuid(row.get("account_id")),
            password_hash: row.get("password_hash"),
            password_changed_at: row.get("password_changed_at"),
        }))
    }

    async fn create_credential(&self, credential: &CommercialCredential) -> AuthResult<()> {
        sqlx::query(
            "INSERT INTO commercial_credential
             (account_id, password_hash, password_changed_at, created_at)
             VALUES ($1, $2, $3, $3)",
        )
        .bind(credential.account_id.as_uuid())
        .bind(&credential.password_hash)
        .bind(credential.password_changed_at)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn is_locked(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<bool> {
        let locked: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM commercial_credential
                WHERE account_id = $1 AND locked_until IS NOT NULL AND locked_until > $2
            )",
        )
        .bind(account_id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        Ok(locked)
    }

    async fn register_failure(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<bool> {
        // Umbral y duración fijos y auditables: 5 fallos, 15 minutos.
        let locked: Option<bool> = sqlx::query_scalar(
            "UPDATE commercial_credential
             SET failed_attempts = failed_attempts + 1,
                 locked_until = CASE
                     WHEN failed_attempts + 1 >= 5 THEN $2 + interval '15 minutes'
                     ELSE locked_until
                 END
             WHERE account_id = $1
             RETURNING failed_attempts >= 5",
        )
        .bind(account_id.as_uuid())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(locked.unwrap_or(false))
    }

    async fn clear_failures(&self, account_id: CommercialAccountId) -> AuthResult<()> {
        sqlx::query(
            "UPDATE commercial_credential
             SET failed_attempts = 0, locked_until = NULL
             WHERE account_id = $1",
        )
        .bind(account_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn create_session(
        &self,
        session: &AuthenticatedSession,
        token_hash: &[u8],
    ) -> AuthResult<()> {
        sqlx::query(
            "INSERT INTO commercial_session
             (id, account_id, token_hash, created_at, expires_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $5, $4)",
        )
        .bind(session.session_id)
        .bind(session.account_id.as_uuid())
        .bind(token_hash)
        .bind(session.created_at)
        .bind(session.expires_at)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn get_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<Option<AuthenticatedSession>> {
        let row = sqlx::query(
            "SELECT s.id, s.account_id, s.created_at, s.expires_at
             FROM commercial_session s
             JOIN commercial_account a ON a.id = s.account_id AND a.status = 'active'
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > $2",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| AuthenticatedSession {
            session_id: row.get("id"),
            account_id: CommercialAccountId::from_uuid(row.get("account_id")),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        }))
    }

    async fn revoke_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<Option<CommercialAccountId>> {
        let account_id: Option<Uuid> = sqlx::query_scalar(
            "UPDATE commercial_session SET revoked_at = $2
             WHERE token_hash = $1 AND revoked_at IS NULL
             RETURNING account_id",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(account_id.map(CommercialAccountId::from_uuid))
    }

    async fn revoke_account_sessions(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<()> {
        sqlx::query(
            "UPDATE commercial_session SET revoked_at = $2
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn record_event(
        &self,
        account_id: Option<CommercialAccountId>,
        action: &str,
        occurred_at: OffsetDateTime,
        metadata: serde_json::Value,
    ) -> AuthResult<()> {
        sqlx::query(
            "INSERT INTO commercial_auth_event (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(account_id.map(|id| id.as_uuid()))
        .bind(action)
        .bind(occurred_at)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }
}
