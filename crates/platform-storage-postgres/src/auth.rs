//! Persistencia de credenciales, sesiones y eventos de autenticación.

use async_trait::async_trait;
use platform_application::auth::{AuthRepository, LoginOutcome, PasswordChangeOutcome};
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
        // El bloqueo de la fila serializa intentos concurrentes y evita que un
        // bloqueo vencido arrastre el contador del ciclo anterior.
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "SELECT failed_attempts, locked_until FROM commercial_credential
             WHERE account_id = $1 FOR UPDATE",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            return Err(AuthError::InvalidCredentials);
        };
        let previous: i32 = row.get("failed_attempts");
        let locked_until: Option<OffsetDateTime> = row.get("locked_until");
        let expired = locked_until.is_some_and(|until| until <= now);
        let attempts = if expired {
            1
        } else {
            previous.saturating_add(1)
        };
        let next_lock = if attempts >= 5 {
            Some(now + time::Duration::minutes(15))
        } else {
            None
        };
        sqlx::query(
            "UPDATE commercial_credential SET failed_attempts = $2, locked_until = $3
             WHERE account_id = $1",
        )
        .bind(account_id.as_uuid())
        .bind(attempts)
        .bind(next_lock)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(attempts >= 5)
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

    async fn rotate_session(
        &self,
        expected_hash: &str,
        session: &AuthenticatedSession,
        token_hash: &[u8],
    ) -> AuthResult<LoginOutcome> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Orden de bloqueo: cuenta -> credencial -> sesiones.
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM commercial_account
             WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(session.account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if active.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Ok(LoginOutcome::AccountUnavailable);
        }

        // Revalida la credencial realmente verificada y el bloqueo vigente.
        let row = sqlx::query(
            "SELECT password_hash, locked_until FROM commercial_credential
             WHERE account_id = $1 FOR UPDATE",
        )
        .bind(session.account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Ok(LoginOutcome::AccountUnavailable);
        };
        let stored_hash: String = row.get("password_hash");
        if stored_hash != expected_hash {
            tx.rollback().await.map_err(storage)?;
            return Ok(LoginOutcome::CredentialChanged);
        }
        // clock_timestamp() es el instante real tras adquirir el bloqueo;
        // now()/transaction_timestamp() sería el inicio de la transacción.
        let db_now: OffsetDateTime = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        // Reloj efectivo: nunca anterior al instante real. El valor inyectado
        // permite pruebas deterministas sin sleeps.
        let effective_now = db_now.max(session.created_at);
        let locked_until: Option<OffsetDateTime> = row.get("locked_until");
        if locked_until.is_some_and(|until| until > effective_now) {
            tx.rollback().await.map_err(storage)?;
            return Ok(LoginOutcome::Locked);
        }

        let ttl = session.expires_at - session.created_at;
        if ttl <= time::Duration::ZERO {
            tx.rollback().await.map_err(storage)?;
            return Err(AuthError::Storage);
        }
        let expires_at = effective_now + ttl;

        sqlx::query(
            "UPDATE commercial_credential
             SET failed_attempts = 0, locked_until = NULL
             WHERE account_id = $1",
        )
        .bind(session.account_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "UPDATE commercial_session SET revoked_at = $2
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(session.account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "INSERT INTO commercial_session
             (id, account_id, token_hash, created_at, expires_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $5, $4)",
        )
        .bind(session.session_id)
        .bind(session.account_id.as_uuid())
        .bind(token_hash)
        .bind(effective_now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "INSERT INTO commercial_auth_event
             (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, 'login_success', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(session.account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        tx.commit().await.map_err(storage)?;
        Ok(LoginOutcome::Rotated { expires_at })
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
            "UPDATE commercial_session
             SET revoked_at = GREATEST($2, clock_timestamp())
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

    async fn change_password(
        &self,
        expected_hash: &str,
        new_hash: &str,
        previous_session_id: Uuid,
        session: &AuthenticatedSession,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> AuthResult<PasswordChangeOutcome> {
        let account_id = session.account_id;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Orden de bloqueo: cuenta -> sesión/credencial. Serializa login,
        // cambio de contraseña y revocación total por cuenta.
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM commercial_account
             WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if active.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Ok(PasswordChangeOutcome::SessionInvalid);
        }

        // Reloj real de PostgreSQL tras adquirir el bloqueo; clock_timestamp()
        // evita transaction_timestamp() anterior a la espera del lock.
        let db_now: OffsetDateTime = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        let effective_now = db_now.max(now);
        // Una revocación ya confirmada invalida la sesión extraída por HTTP.
        let session_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM commercial_session
                WHERE id = $1 AND account_id = $2
                  AND revoked_at IS NULL AND expires_at > $3)",
        )
        .bind(previous_session_id)
        .bind(account_id.as_uuid())
        .bind(effective_now)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if !session_valid {
            tx.rollback().await.map_err(storage)?;
            return Ok(PasswordChangeOutcome::SessionInvalid);
        }

        // Comparación optimista: si otra operación cambió el hash, no se toca nada.
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE commercial_credential
             SET password_hash = $3, password_changed_at = $4,
                 failed_attempts = 0, locked_until = NULL
             WHERE account_id = $1 AND password_hash = $2
             RETURNING account_id",
        )
        .bind(account_id.as_uuid())
        .bind(expected_hash)
        .bind(new_hash)
        .bind(effective_now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if updated.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Ok(PasswordChangeOutcome::CredentialChanged);
        }

        let ttl = session.expires_at - session.created_at;
        if ttl <= time::Duration::ZERO {
            tx.rollback().await.map_err(storage)?;
            return Err(AuthError::Storage);
        }
        let expires_at = effective_now + ttl;

        sqlx::query(
            "UPDATE commercial_session SET revoked_at = $2
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "INSERT INTO commercial_session
             (id, account_id, token_hash, created_at, expires_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $5, $4)",
        )
        .bind(session.session_id)
        .bind(session.account_id.as_uuid())
        .bind(token_hash)
        .bind(effective_now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "INSERT INTO commercial_auth_event
             (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, 'password_changed', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        tx.commit().await.map_err(storage)?;
        Ok(PasswordChangeOutcome::Updated { expires_at })
    }

    async fn revoke_all_sessions(
        &self,
        account_id: CommercialAccountId,
        now: OffsetDateTime,
    ) -> AuthResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Mismo orden de bloqueo que login y cambio de contraseña.
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM commercial_account WHERE id = $1 FOR UPDATE")
                .bind(account_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        if exists.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Err(AuthError::InvalidSession);
        }
        let db_now: OffsetDateTime = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        let effective_now = db_now.max(now);
        sqlx::query(
            "UPDATE commercial_session SET revoked_at = $2
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        sqlx::query(
            "INSERT INTO commercial_auth_event
             (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, 'session_revoked', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
}
