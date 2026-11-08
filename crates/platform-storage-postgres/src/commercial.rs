//! Consultas comerciales aisladas por actor verificado y organización.

use async_trait::async_trait;
use base64::Engine as _;
use platform_application::commercial::{CommercialRepository, OrganizationView, Page};
use platform_core::{
    commercial::{
        CommercialAccount, CommercialError, CommercialOrganization, CommercialResult,
        OrganizationMembership, OrganizationRole,
    },
    ids::{CommercialAccountId, InstallationId, OrganizationId},
    installation::Installation,
};
use serde::{Deserialize, Serialize};
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

/// Clave de orden del listado de organizaciones: (display_name, id).
#[derive(Debug, Serialize, Deserialize)]
struct OrganizationCursor {
    n: String,
    i: Uuid,
}

/// Clave de orden del listado de miembros: (joined_at nanos, account_id).
#[derive(Debug, Serialize, Deserialize)]
struct MemberCursor {
    t: i64,
    i: Uuid,
}

fn encode_cursor<T: Serialize>(payload: &T) -> CommercialResult<String> {
    let json = serde_json::to_vec(payload).map_err(|_| CommercialError::InvalidCursor)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

fn decode_cursor<T: for<'de> Deserialize<'de>>(cursor: &str) -> CommercialResult<T> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| CommercialError::InvalidCursor)?;
    serde_json::from_slice(&bytes).map_err(|_| CommercialError::InvalidCursor)
}

fn decode_organization_cursor(cursor: &str) -> CommercialResult<(String, OrganizationId)> {
    let decoded: OrganizationCursor = decode_cursor(cursor)?;
    Ok((decoded.n, OrganizationId::from_uuid(decoded.i)))
}

fn decode_member_cursor(cursor: &str) -> CommercialResult<(OffsetDateTime, CommercialAccountId)> {
    let decoded: MemberCursor = decode_cursor(cursor)?;
    let joined_at = OffsetDateTime::from_unix_timestamp_nanos(i128::from(decoded.t))
        .map_err(|_| CommercialError::InvalidCursor)?;
    Ok((joined_at, CommercialAccountId::from_uuid(decoded.i)))
}

fn page_limit(limit: u32) -> i64 {
    i64::from(limit.max(1)) + 1
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

/// Comprueba la membresía activa del actor y devuelve su rol, bloqueando la
/// organización para serializar mutaciones de membresía.
async fn require_membership(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
) -> CommercialResult<OrganizationRole> {
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
    let membership: Option<(String, i16)> = sqlx::query_as(
        "SELECT m.role, m.role_version
         FROM organization_membership m
         JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
         WHERE m.organization_id = $1 AND m.account_id = $2
           AND m.revoked_at IS NULL",
    )
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    let Some((role, version)) = membership else {
        return Err(CommercialError::NotFound);
    };
    OrganizationRole::from_storage(&role, version)
}

/// Sólo `owner` administra membresías; otros roles reciben `Forbidden`.
async fn require_owner(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
) -> CommercialResult<()> {
    match require_membership(tx, actor, organization).await? {
        OrganizationRole::Owner => Ok(()),
        OrganizationRole::Admin | OrganizationRole::Member => Err(CommercialError::Forbidden),
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
        // Bloqueo de cuenta: una deshabilitación confirmada antes de crear no
        // puede ser ignorada por un estado leído fuera de la transacción.
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM commercial_account
             WHERE id = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(actor.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if active.is_none() {
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
        // Bloqueo compartido del sujeto: una deshabilitación concurrente no
        // puede confirmarse entre esta comprobación y el alta.
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM commercial_account
             WHERE id = $1 AND status = 'active' FOR SHARE",
        )
        .bind(member.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if active.is_none() {
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
            "UPDATE organization_membership
             SET revoked_at = GREATEST($3, joined_at)
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
        after: Option<&str>,
        limit: u32,
    ) -> CommercialResult<Page<OrganizationMembership>> {
        // La autorización se resuelve antes de leer; un id ajeno nunca enumera.
        let actor_role: Option<(String, i16)> = sqlx::query_as(
            "SELECT m.role, m.role_version
             FROM organization_membership m
             JOIN organization o ON o.id = m.organization_id AND o.status = 'active'
             JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
             WHERE m.organization_id = $1 AND m.account_id = $2
               AND m.revoked_at IS NULL",
        )
        .bind(organization.as_uuid())
        .bind(actor.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some((role, version)) = actor_role else {
            return Err(CommercialError::NotFound);
        };
        OrganizationRole::from_storage(&role, version)?;

        let rows = if let Some(cursor) = after {
            let (joined_at, account_id) = decode_member_cursor(cursor)?;
            sqlx::query(
                "SELECT m.account_id, m.role, m.role_version, m.joined_at
                 FROM organization_membership m
                 WHERE m.organization_id = $1 AND m.revoked_at IS NULL
                   AND (m.joined_at, m.account_id) > ($2, $3)
                 ORDER BY m.joined_at, m.account_id
                 LIMIT $4",
            )
            .bind(organization.as_uuid())
            .bind(joined_at)
            .bind(account_id.as_uuid())
            .bind(page_limit(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?
        } else {
            sqlx::query(
                "SELECT m.account_id, m.role, m.role_version, m.joined_at
                 FROM organization_membership m
                 WHERE m.organization_id = $1 AND m.revoked_at IS NULL
                 ORDER BY m.joined_at, m.account_id
                 LIMIT $2",
            )
            .bind(organization.as_uuid())
            .bind(page_limit(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?
        };

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row.get("role");
            let version: i16 = row.get("role_version");
            items.push(OrganizationMembership {
                organization_id: organization,
                account_id: CommercialAccountId::from_uuid(row.get("account_id")),
                role: OrganizationRole::from_storage(&role, version)?,
                joined_at: row.get("joined_at"),
            });
        }
        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }
        let next_cursor = if has_more {
            items
                .last()
                .map(|last| {
                    encode_cursor(&MemberCursor {
                        t: last.joined_at.unix_timestamp_nanos() as i64,
                        i: last.account_id.as_uuid(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(Page::new(items, next_cursor))
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

    async fn list_organizations(
        &self,
        actor: CommercialAccountId,
        after: Option<&str>,
        limit: u32,
    ) -> CommercialResult<Page<OrganizationView>> {
        // Aislamiento en SQL: cuenta y membresía activas, organización activa.
        let rows = if let Some(cursor) = after {
            let (display_name, id) = decode_organization_cursor(cursor)?;
            sqlx::query(
                "SELECT o.id, o.display_name, m.role, m.role_version
                 FROM organization_membership m
                 JOIN organization o ON o.id = m.organization_id AND o.status = 'active'
                 JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
                 WHERE m.account_id = $1 AND m.revoked_at IS NULL
                   AND (o.display_name, o.id) > ($2, $3)
                 ORDER BY o.display_name, o.id
                 LIMIT $4",
            )
            .bind(actor.as_uuid())
            .bind(display_name)
            .bind(id.as_uuid())
            .bind(page_limit(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?
        } else {
            sqlx::query(
                "SELECT o.id, o.display_name, m.role, m.role_version
                 FROM organization_membership m
                 JOIN organization o ON o.id = m.organization_id AND o.status = 'active'
                 JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
                 WHERE m.account_id = $1 AND m.revoked_at IS NULL
                 ORDER BY o.display_name, o.id
                 LIMIT $2",
            )
            .bind(actor.as_uuid())
            .bind(page_limit(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?
        };

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row.get("role");
            let version: i16 = row.get("role_version");
            items.push(OrganizationView {
                id: OrganizationId::from_uuid(row.get("id")),
                display_name: row.get("display_name"),
                role: OrganizationRole::from_storage(&role, version)?,
                status: "active",
            });
        }
        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }
        let next_cursor = if has_more {
            items
                .last()
                .map(|last| {
                    encode_cursor(&OrganizationCursor {
                        n: last.display_name.clone(),
                        i: last.id.as_uuid(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(Page::new(items, next_cursor))
    }

    async fn get_organization(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
    ) -> CommercialResult<OrganizationView> {
        let row = sqlx::query(
            "SELECT o.id, o.display_name, m.role, m.role_version
             FROM organization_membership m
             JOIN organization o ON o.id = m.organization_id AND o.status = 'active'
             JOIN commercial_account a ON a.id = m.account_id AND a.status = 'active'
             WHERE m.account_id = $1 AND m.organization_id = $2
               AND m.revoked_at IS NULL",
        )
        .bind(actor.as_uuid())
        .bind(organization.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            return Err(CommercialError::NotFound);
        };
        let role: String = row.get("role");
        let version: i16 = row.get("role_version");
        Ok(OrganizationView {
            id: OrganizationId::from_uuid(row.get("id")),
            display_name: row.get("display_name"),
            role: OrganizationRole::from_storage(&role, version)?,
            status: "active",
        })
    }
}
