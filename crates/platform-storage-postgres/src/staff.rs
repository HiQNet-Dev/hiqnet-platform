//! Persistencia de identidad, credenciales, sesiones y eventos de personal.

use async_trait::async_trait;
use platform_application::staff::{StaffLoginOutcome, StaffRepository};
use platform_core::{
    ids::StaffAccountId,
    staff::{
        AuthenticatedStaffSession, StaffCredential, StaffError, StaffIdentity, StaffResult,
        StaffRole,
    },
};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgStaffRepository {
    pool: PgPool,
}

impl PgStaffRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Crea cuenta, credencial y rol activo en una sola transacción. Sólo para
    /// bootstrap DEV o alta administrativa auditada; no es un registro público.
    pub async fn create_staff(
        &self,
        email: &str,
        display_name: &str,
        role: StaffRole,
        password_hash: &str,
        now: OffsetDateTime,
    ) -> StaffResult<StaffAccountId> {
        let account_id = StaffAccountId::new();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query(
            "INSERT INTO staff_account (id, email, display_name, status, created_at, updated_at)
             VALUES ($1, $2, $3, 'active', $4, $4)",
        )
        .bind(account_id.as_uuid())
        .bind(email.trim().to_lowercase())
        .bind(display_name.trim())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        sqlx::query(
            "INSERT INTO staff_credential
             (account_id, password_hash, password_changed_at, created_at)
             VALUES ($1, $2, $3, $3)",
        )
        .bind(account_id.as_uuid())
        .bind(password_hash)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        sqlx::query(
            "INSERT INTO staff_role_assignment (id, account_id, role, granted_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(account_id.as_uuid())
        .bind(role.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(account_id)
    }
}

fn storage(_error: sqlx::Error) -> StaffError {
    StaffError::Storage
}

fn role_from_row(row: &sqlx::postgres::PgRow) -> StaffResult<StaffRole> {
    let value: String = row.get("role");
    value.parse().map_err(|_| StaffError::Storage)
}

#[async_trait]
impl StaffRepository for PgStaffRepository {
    async fn find_identity_by_email(&self, email: &str) -> StaffResult<Option<StaffIdentity>> {
        let email = email.trim().to_lowercase();
        let row = sqlx::query(
            "SELECT a.id, a.email, a.display_name, r.role
             FROM staff_account a
             JOIN staff_role_assignment r ON r.account_id = a.id AND r.revoked_at IS NULL
             WHERE a.email = $1 AND a.status = 'active'",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.map(|row| {
            Ok(StaffIdentity {
                account_id: StaffAccountId::from_uuid(row.get("id")),
                email: row.get("email"),
                display_name: row.get("display_name"),
                role: role_from_row(&row)?,
            })
        })
        .transpose()
    }

    async fn get_identity(&self, account_id: StaffAccountId) -> StaffResult<Option<StaffIdentity>> {
        let row = sqlx::query(
            "SELECT a.id, a.email, a.display_name, r.role
             FROM staff_account a
             JOIN staff_role_assignment r ON r.account_id = a.id AND r.revoked_at IS NULL
             WHERE a.id = $1 AND a.status = 'active'",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.map(|row| {
            Ok(StaffIdentity {
                account_id: StaffAccountId::from_uuid(row.get("id")),
                email: row.get("email"),
                display_name: row.get("display_name"),
                role: role_from_row(&row)?,
            })
        })
        .transpose()
    }

    async fn get_credential(
        &self,
        account_id: StaffAccountId,
    ) -> StaffResult<Option<StaffCredential>> {
        let row = sqlx::query(
            "SELECT account_id, password_hash, password_changed_at
             FROM staff_credential WHERE account_id = $1",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| StaffCredential {
            account_id: StaffAccountId::from_uuid(row.get("account_id")),
            password_hash: row.get("password_hash"),
            password_changed_at: row.get("password_changed_at"),
        }))
    }

    async fn create_credential(&self, credential: &StaffCredential) -> StaffResult<()> {
        sqlx::query(
            "INSERT INTO staff_credential
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
        account_id: StaffAccountId,
        now: OffsetDateTime,
    ) -> StaffResult<bool> {
        let locked: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM staff_credential
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
        account_id: StaffAccountId,
        now: OffsetDateTime,
    ) -> StaffResult<bool> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "SELECT failed_attempts, locked_until FROM staff_credential
             WHERE account_id = $1 FOR UPDATE",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            return Err(StaffError::InvalidCredentials);
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
            "UPDATE staff_credential SET failed_attempts = $2, locked_until = $3
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

    async fn rotate_session(
        &self,
        expected_hash: &str,
        session: &AuthenticatedStaffSession,
        token_hash: &[u8],
    ) -> StaffResult<StaffLoginOutcome> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Orden de bloqueo: cuenta -> credencial -> sesiones.
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM staff_account
             WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(session.account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if active.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Ok(StaffLoginOutcome::AccountUnavailable);
        }

        let row = sqlx::query(
            "SELECT password_hash, locked_until FROM staff_credential
             WHERE account_id = $1 FOR UPDATE",
        )
        .bind(session.account_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Ok(StaffLoginOutcome::AccountUnavailable);
        };
        let stored_hash: String = row.get("password_hash");
        if stored_hash != expected_hash {
            tx.rollback().await.map_err(storage)?;
            return Ok(StaffLoginOutcome::CredentialChanged);
        }
        let db_now: OffsetDateTime = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        let effective_now = db_now.max(session.created_at);
        let locked_until: Option<OffsetDateTime> = row.get("locked_until");
        if locked_until.is_some_and(|until| until > effective_now) {
            tx.rollback().await.map_err(storage)?;
            return Ok(StaffLoginOutcome::Locked);
        }

        let ttl = session.expires_at - session.created_at;
        if ttl <= time::Duration::ZERO {
            tx.rollback().await.map_err(storage)?;
            return Err(StaffError::Storage);
        }
        let expires_at = effective_now + ttl;

        sqlx::query(
            "UPDATE staff_credential
             SET failed_attempts = 0, locked_until = NULL
             WHERE account_id = $1",
        )
        .bind(session.account_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "UPDATE staff_session
             SET revoked_at = GREATEST($2, clock_timestamp(), created_at)
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(session.account_id.as_uuid())
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;

        sqlx::query(
            "INSERT INTO staff_session
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
            "INSERT INTO staff_auth_event
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
        Ok(StaffLoginOutcome::Rotated { expires_at })
    }

    async fn get_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> StaffResult<Option<AuthenticatedStaffSession>> {
        let row = sqlx::query(
            "SELECT s.id, s.account_id, s.created_at, s.expires_at
             FROM staff_session s
             JOIN staff_account a ON a.id = s.account_id AND a.status = 'active'
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > $2
               AND EXISTS (
                   SELECT 1 FROM staff_role_assignment r
                   WHERE r.account_id = a.id AND r.revoked_at IS NULL
               )",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| AuthenticatedStaffSession {
            session_id: row.get("id"),
            account_id: StaffAccountId::from_uuid(row.get("account_id")),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        }))
    }

    async fn revoke_session(
        &self,
        token_hash: &[u8],
        now: OffsetDateTime,
    ) -> StaffResult<Option<StaffAccountId>> {
        let account_id: Option<Uuid> = sqlx::query_scalar(
            "UPDATE staff_session
             SET revoked_at = GREATEST($2, clock_timestamp(), created_at)
             WHERE token_hash = $1 AND revoked_at IS NULL
             RETURNING account_id",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(account_id.map(StaffAccountId::from_uuid))
    }

    async fn record_event(
        &self,
        account_id: Option<StaffAccountId>,
        action: &str,
        occurred_at: OffsetDateTime,
        metadata: serde_json::Value,
    ) -> StaffResult<()> {
        sqlx::query(
            "INSERT INTO staff_auth_event (id, account_id, action, occurred_at, metadata)
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
