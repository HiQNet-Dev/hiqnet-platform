//! Consultas comerciales aisladas por actor verificado y organización.

use async_trait::async_trait;
use platform_application::commercial::CommercialRepository;
use platform_core::{
    commercial::{
        CommercialAccount, CommercialError, CommercialOrganization, CommercialResult,
        OrganizationMembership, OrganizationRole,
    },
    ids::{CommercialAccountId, InstallationId, OrganizationId},
    installation::Installation,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgCommercialRepository {
    pool: PgPool,
}

impl PgCommercialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> CommercialError {
    CommercialError::Storage
}

fn insertion(error: sqlx::Error) -> CommercialError {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => CommercialError::Conflict,
        _ => CommercialError::Storage,
    }
}

async fn require_owner(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
) -> CommercialResult<()> {
    // Serializa cambios de membresía y protección del último owner por organización.
    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM organization WHERE id = $1 AND status = 'active' FOR UPDATE",
    )
    .bind(organization.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    if active.is_none() {
        return Err(CommercialError::NotFound);
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM organization_membership m
            JOIN commercial_account a ON a.id = m.account_id
            WHERE m.organization_id = $1 AND m.account_id = $2
              AND m.role = 'owner' AND m.role_version = 1
              AND m.revoked_at IS NULL AND a.status = 'active')",
    )
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .fetch_one(&mut **tx)
    .await
    .map_err(storage)?;
    if allowed {
        Ok(())
    } else {
        Err(CommercialError::NotFound)
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
    subject: CommercialAccountId,
    action: &str,
    roles: (Option<OrganizationRole>, Option<OrganizationRole>),
    now: OffsetDateTime,
) -> CommercialResult<()> {
    sqlx::query(
        "INSERT INTO commercial_audit_event
         (id, organization_id, actor_account_id, subject_account_id, action, old_role, new_role, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .bind(subject.as_uuid())
    .bind(action)
    .bind(roles.0.map(OrganizationRole::as_str))
    .bind(roles.1.map(OrganizationRole::as_str))
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

async fn member_role(
    tx: &mut Transaction<'_, Postgres>,
    organization: OrganizationId,
    member: CommercialAccountId,
) -> CommercialResult<OrganizationRole> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT role FROM organization_membership
         WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL",
    )
    .bind(organization.as_uuid())
    .bind(member.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    let Some(current) = current else {
        return Err(CommercialError::NotFound);
    };
    OrganizationRole::from_storage(&current, OrganizationRole::VERSION)
}

async fn protect_last_owner(
    tx: &mut Transaction<'_, Postgres>,
    organization: OrganizationId,
    member: CommercialAccountId,
) -> CommercialResult<OrganizationRole> {
    let current = member_role(tx, organization, member).await?;
    if current == OrganizationRole::Owner {
        let owners: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_membership m
             JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
             WHERE m.organization_id = $1 AND m.role = 'owner' AND m.revoked_at IS NULL",
        )
        .bind(organization.as_uuid())
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)?;
        if owners <= 1 {
            return Err(CommercialError::Conflict);
        }
    }
    Ok(current)
}

#[async_trait]
impl CommercialRepository for PgCommercialRepository {
    async fn create_account(&self, account: &CommercialAccount) -> CommercialResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query(
            "INSERT INTO commercial_account (id, email, display_name, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(account.id.as_uuid())
        .bind(&account.email)
        .bind(&account.display_name)
        .bind(account.created_at)
        .execute(&mut *tx)
        .await
        .map_err(insertion)?;
        sqlx::query(
            "INSERT INTO commercial_audit_event
             (id, subject_account_id, action, occurred_at)
             VALUES ($1, $2, 'account_created', $3)",
        )
        .bind(Uuid::new_v4())
        .bind(account.id.as_uuid())
        .bind(account.created_at)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }

    async fn create_organization(
        &self,
        actor: CommercialAccountId,
        organization: &CommercialOrganization,
    ) -> CommercialResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM commercial_account WHERE id = $1 AND status = 'active')",
        )
        .bind(actor.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if !active {
            return Err(CommercialError::NotFound);
        }
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, $2, 'active', $3)",
        )
        .bind(organization.id.as_uuid())
        .bind(&organization.display_name)
        .bind(organization.created_at)
        .execute(&mut *tx)
        .await
        .map_err(insertion)?;
        sqlx::query(
            "INSERT INTO organization_membership
             (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, 'owner', 1, $3)",
        )
        .bind(organization.id.as_uuid())
        .bind(actor.as_uuid())
        .bind(organization.created_at)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        audit(
            &mut tx,
            actor,
            organization.id,
            actor,
            "organization_created",
            (None, None),
            organization.created_at,
        )
        .await?;
        audit(
            &mut tx,
            actor,
            organization.id,
            actor,
            "membership_added",
            (None, Some(OrganizationRole::Owner)),
            organization.created_at,
        )
        .await?;
        tx.commit().await.map_err(storage)
    }

    async fn add_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        require_owner(&mut tx, actor, organization).await?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM commercial_account WHERE id = $1 AND status = 'active')",
        )
        .bind(member.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if !active {
            return Err(CommercialError::NotFound);
        }
        sqlx::query(
            "INSERT INTO organization_membership
             (organization_id, account_id, role, role_version, joined_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(organization.as_uuid())
        .bind(member.as_uuid())
        .bind(role.as_str())
        .bind(OrganizationRole::VERSION)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(insertion)?;
        audit(
            &mut tx,
            actor,
            organization,
            member,
            "membership_added",
            (None, Some(role)),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)
    }

    async fn change_role(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        role: OrganizationRole,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        require_owner(&mut tx, actor, organization).await?;
        let previous = if role != OrganizationRole::Owner {
            protect_last_owner(&mut tx, organization, member).await?
        } else {
            member_role(&mut tx, organization, member).await?
        };
        if previous == role {
            return Err(CommercialError::Conflict);
        }
        let changed = sqlx::query(
            "UPDATE organization_membership SET role = $3, role_version = $4
             WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL",
        )
        .bind(organization.as_uuid())
        .bind(member.as_uuid())
        .bind(role.as_str())
        .bind(OrganizationRole::VERSION)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        if changed.rows_affected() == 0 {
            return Err(CommercialError::NotFound);
        }
        audit(
            &mut tx,
            actor,
            organization,
            member,
            "membership_role_changed",
            (Some(previous), Some(role)),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)
    }

    async fn revoke_member(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        member: CommercialAccountId,
        now: OffsetDateTime,
    ) -> CommercialResult<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        require_owner(&mut tx, actor, organization).await?;
        let previous = protect_last_owner(&mut tx, organization, member).await?;
        let changed = sqlx::query(
            "UPDATE organization_membership SET revoked_at = $3
             WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL",
        )
        .bind(organization.as_uuid())
        .bind(member.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        if changed.rows_affected() == 0 {
            return Err(CommercialError::NotFound);
        }
        audit(
            &mut tx,
            actor,
            organization,
            member,
            "membership_revoked",
            (Some(previous), None),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)
    }

    async fn list_members(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<Vec<OrganizationMembership>> {
        // Un id de organización conocido no permite enumerar miembros ajenos.
        let rows = sqlx::query(
            "SELECT m.account_id, m.role, m.role_version, m.joined_at
             FROM organization_membership m
             JOIN organization o ON o.id = m.organization_id AND o.status = 'active'
             WHERE m.organization_id = $1 AND m.revoked_at IS NULL
               AND EXISTS (
                 SELECT 1 FROM organization_membership mine
                 JOIN commercial_account a ON a.id = mine.account_id
                 WHERE mine.organization_id = m.organization_id AND mine.account_id = $2
                   AND mine.revoked_at IS NULL AND a.status = 'active')
             ORDER BY m.joined_at, m.account_id",
        )
        .bind(organization.as_uuid())
        .bind(actor.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        if rows.is_empty() {
            return Err(CommercialError::NotFound);
        }
        rows.into_iter()
            .map(|row| {
                let role: String = row.get("role");
                let version: i16 = row.get("role_version");
                Ok(OrganizationMembership {
                    organization_id: organization,
                    account_id: CommercialAccountId::from_uuid(row.get("account_id")),
                    role: OrganizationRole::from_storage(&role, version)?,
                    joined_at: row.get("joined_at"),
                })
            })
            .collect()
    }

    async fn get_installation(
        &self,
        actor: CommercialAccountId,
        installation: InstallationId,
    ) -> CommercialResult<Installation> {
        let row = sqlx::query(
            "SELECT i.id, i.organization_id, i.device_label, i.status, i.registered_at,
                    i.last_seen_at, i.trial_started_at, i.trial_ends_at
             FROM installation i
             JOIN organization o ON o.id = i.organization_id AND o.status = 'active'
             JOIN organization_membership m
               ON m.organization_id = o.id AND m.account_id = $2 AND m.revoked_at IS NULL
             JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
             WHERE i.id = $1",
        )
        .bind(installation.as_uuid())
        .bind(actor.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.map(super::repository::row_to_installation)
            .ok_or(CommercialError::NotFound)
    }
}
