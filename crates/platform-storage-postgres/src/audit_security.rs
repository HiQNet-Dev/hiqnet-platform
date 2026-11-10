//! Adaptador PostgreSQL de los read models de Audit y Security Center.

use async_trait::async_trait;
use platform_application::audit_security::{
    AuditEventDetail, AuditEventSummary, AuditFilters, AuditResult, AuditSecurityError,
    AuditSecurityRepository, AuditSecurityResult, CommercialAccountDetail, CommercialAccountRef,
    CursorKey, InstallationRef, OrganizationRef, ProductRef, SecurityEventDetail,
    SecurityEventSummary, SecurityFilters, SecurityKind, StaffActorDetail, StaffActorRef,
    StaffAuthAction, StaffAuthEventDetail, StaffAuthEventSummary, StaffAuthFilters,
};
use platform_core::ids::{
    CommercialAccountId, InstallationId, OrganizationId, ProductId, StaffAccountId,
};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::str::FromStr;
use uuid::Uuid;

pub struct PgAuditSecurityRepository {
    pool: PgPool,
}

impl PgAuditSecurityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> AuditSecurityError {
    AuditSecurityError::Storage
}

fn staff_actor(row: &PgRow) -> AuditSecurityResult<Option<StaffActorRef>> {
    let id: Option<Uuid> = row.try_get("actor_id").map_err(storage)?;
    let name: Option<String> = row.try_get("actor_name").map_err(storage)?;
    Ok(id.zip(name).map(|(id, display_name)| StaffActorRef {
        account_id: StaffAccountId::from_uuid(id),
        display_name,
    }))
}

fn staff_actor_detail(row: &PgRow) -> AuditSecurityResult<Option<StaffActorDetail>> {
    let id: Option<Uuid> = row.try_get("actor_id").map_err(storage)?;
    let name: Option<String> = row.try_get("actor_name").map_err(storage)?;
    let email: Option<String> = row.try_get("actor_email").map_err(storage)?;
    let status: Option<String> = row.try_get("actor_status").map_err(storage)?;
    Ok(id
        .zip(name)
        .zip(email)
        .zip(status)
        .map(|(((id, display_name), email), status)| StaffActorDetail {
            account_id: StaffAccountId::from_uuid(id),
            display_name,
            email,
            status,
        }))
}

fn organization(row: &PgRow) -> AuditSecurityResult<Option<OrganizationRef>> {
    let id: Option<Uuid> = row.try_get("org_id").map_err(storage)?;
    let name: Option<String> = row.try_get("org_name").map_err(storage)?;
    Ok(id.zip(name).map(|(id, display_name)| OrganizationRef {
        id: OrganizationId::from_uuid(id),
        display_name,
    }))
}

fn product(row: &PgRow) -> AuditSecurityResult<Option<ProductRef>> {
    let id: Option<Uuid> = row.try_get("product_id").map_err(storage)?;
    let key: Option<String> = row.try_get("product_key").map_err(storage)?;
    let name: Option<String> = row.try_get("product_name").map_err(storage)?;
    Ok(id.zip(key).zip(name).map(|((id, key), name)| ProductRef {
        id: ProductId::from_uuid(id),
        key,
        name,
    }))
}

fn commercial_account(row: &PgRow) -> AuditSecurityResult<Option<CommercialAccountRef>> {
    let id: Option<Uuid> = row.try_get("account_id").map_err(storage)?;
    let name: Option<String> = row.try_get("account_name").map_err(storage)?;
    Ok(id.zip(name).map(|(id, display_name)| CommercialAccountRef {
        id: CommercialAccountId::from_uuid(id),
        display_name,
    }))
}

fn commercial_account_detail(row: &PgRow) -> AuditSecurityResult<Option<CommercialAccountDetail>> {
    let id: Option<Uuid> = row.try_get("account_id").map_err(storage)?;
    let name: Option<String> = row.try_get("account_name").map_err(storage)?;
    let email: Option<String> = row.try_get("account_email").map_err(storage)?;
    Ok(id
        .zip(name)
        .zip(email)
        .map(|((id, display_name), email)| CommercialAccountDetail {
            id: CommercialAccountId::from_uuid(id),
            display_name,
            email,
        }))
}

fn installation(row: &PgRow) -> AuditSecurityResult<Option<InstallationRef>> {
    let id: Option<Uuid> = row.try_get("installation_id").map_err(storage)?;
    let label: Option<String> = row.try_get("installation_label").map_err(storage)?;
    Ok(id.zip(label).map(|(id, device_label)| InstallationRef {
        id: InstallationId::from_uuid(id),
        device_label,
    }))
}

fn audit_summary(row: &PgRow) -> AuditSecurityResult<AuditEventSummary> {
    let result: String = row.try_get("result").map_err(storage)?;
    Ok(AuditEventSummary {
        id: row.try_get("id").map_err(storage)?,
        occurred_at: row.try_get("occurred_at").map_err(storage)?,
        actor: staff_actor(row)?,
        action: row.try_get("action").map_err(storage)?,
        resource_type: row.try_get("resource_type").map_err(storage)?,
        resource_id: row.try_get("resource_id").map_err(storage)?,
        resource_label: row.try_get("resource_label").map_err(storage)?,
        organization: organization(row)?,
        product: product(row)?,
        result: AuditResult::from_str(&result).map_err(|_| AuditSecurityError::Storage)?,
    })
}

fn security_summary(row: &PgRow) -> AuditSecurityResult<SecurityEventSummary> {
    let kind: String = row.try_get("kind").map_err(storage)?;
    Ok(SecurityEventSummary {
        id: row.try_get("id").map_err(storage)?,
        occurred_at: row.try_get("occurred_at").map_err(storage)?,
        kind: SecurityKind::from_str(&kind).map_err(|_| AuditSecurityError::Storage)?,
        organization: organization(row)?,
        installation: installation(row)?,
        account: commercial_account(row)?,
    })
}

fn staff_auth_summary(row: &PgRow) -> AuditSecurityResult<StaffAuthEventSummary> {
    let action: String = row.try_get("action").map_err(storage)?;
    Ok(StaffAuthEventSummary {
        id: row.try_get("id").map_err(storage)?,
        occurred_at: row.try_get("occurred_at").map_err(storage)?,
        action: StaffAuthAction::from_str(&action).map_err(|_| AuditSecurityError::Storage)?,
        account: staff_actor(row)?,
    })
}

const AUDIT_SELECT: &str = r#"
SELECT e.id, e.occurred_at, e.action, e.resource_type, e.resource_id,
       e.result,
       a.id AS actor_id, a.display_name AS actor_name,
       o.id AS org_id, o.display_name AS org_name,
       p.id AS product_id, p.key AS product_key, p.name AS product_name,
       CASE e.resource_type
         WHEN 'product' THEN (SELECT name FROM product WHERE id = e.resource_id)
         WHEN 'project' THEN (SELECT name FROM project WHERE id = e.resource_id)
         WHEN 'installation' THEN (SELECT device_label FROM installation WHERE id = e.resource_id)
         ELSE NULL
       END AS resource_label
FROM staff_audit_event e
LEFT JOIN staff_account a ON a.id = e.staff_account_id
LEFT JOIN organization o ON o.id = e.organization_id
LEFT JOIN product p ON p.id = e.product_id
"#;

const AUDIT_DETAIL_SELECT: &str = r#"
SELECT e.id, e.occurred_at, e.action, e.resource_type, e.resource_id,
       e.result, e.metadata,
       a.id AS actor_id, a.display_name AS actor_name, a.email AS actor_email, a.status AS actor_status,
       o.id AS org_id, o.display_name AS org_name,
       p.id AS product_id, p.key AS product_key, p.name AS product_name,
       CASE e.resource_type
         WHEN 'product' THEN (SELECT name FROM product WHERE id = e.resource_id)
         WHEN 'project' THEN (SELECT name FROM project WHERE id = e.resource_id)
         WHEN 'installation' THEN (SELECT device_label FROM installation WHERE id = e.resource_id)
         ELSE NULL
       END AS resource_label
FROM staff_audit_event e
LEFT JOIN staff_account a ON a.id = e.staff_account_id
LEFT JOIN organization o ON o.id = e.organization_id
LEFT JOIN product p ON p.id = e.product_id
"#;

const SECURITY_SELECT: &str = r#"
SELECT e.id, e.created_at AS occurred_at, e.kind,
       o.id AS org_id, o.display_name AS org_name,
       i.id AS installation_id, i.device_label AS installation_label,
       c.id AS account_id, c.display_name AS account_name
FROM commercial_security_event e
LEFT JOIN organization o ON o.id = e.organization_id
LEFT JOIN installation i ON i.id = e.installation_id
LEFT JOIN commercial_account c ON c.id = e.account_id
"#;

const SECURITY_DETAIL_SELECT: &str = r#"
SELECT e.id, e.created_at AS occurred_at, e.kind, e.metadata,
       o.id AS org_id, o.display_name AS org_name,
       i.id AS installation_id, i.device_label AS installation_label,
       c.id AS account_id, c.display_name AS account_name, c.email AS account_email
FROM commercial_security_event e
LEFT JOIN organization o ON o.id = e.organization_id
LEFT JOIN installation i ON i.id = e.installation_id
LEFT JOIN commercial_account c ON c.id = e.account_id
"#;

const STAFF_AUTH_SELECT: &str = r#"
SELECT e.id, e.occurred_at, e.action,
       a.id AS actor_id, a.display_name AS actor_name
FROM staff_auth_event e
LEFT JOIN staff_account a ON a.id = e.account_id
"#;

const STAFF_AUTH_DETAIL_SELECT: &str = r#"
SELECT e.id, e.occurred_at, e.action, e.metadata,
       a.id AS actor_id, a.display_name AS actor_name, a.email AS actor_email, a.status AS actor_status
FROM staff_auth_event e
LEFT JOIN staff_account a ON a.id = e.account_id
"#;

#[async_trait]
impl AuditSecurityRepository for PgAuditSecurityRepository {
    async fn list_audit(
        &self,
        filters: &AuditFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<AuditEventSummary>> {
        let sql = format!(
            "{AUDIT_SELECT}
             WHERE ($1::uuid IS NULL OR e.staff_account_id = $1)
               AND ($2::text IS NULL OR e.action = $2)
               AND ($3::text IS NULL OR e.resource_type = $3)
               AND ($4::uuid IS NULL OR e.resource_id = $4)
               AND ($5::uuid IS NULL OR e.organization_id = $5)
               AND ($6::uuid IS NULL OR e.product_id = $6)
               AND ($7::text IS NULL OR e.result = $7)
               AND ($8::timestamptz IS NULL OR e.occurred_at >= $8)
               AND ($9::timestamptz IS NULL OR e.occurred_at <= $9)
               AND ($10::timestamptz IS NULL OR (e.occurred_at, e.id) < ($10, $11::uuid))
             ORDER BY e.occurred_at DESC, e.id DESC
             LIMIT $12"
        );
        let rows = sqlx::query(&sql)
            .bind(filters.staff_account_id.map(|id| id.as_uuid()))
            .bind(filters.action.as_deref())
            .bind(filters.resource_type.as_deref())
            .bind(filters.resource_id)
            .bind(filters.organization_id.map(|id| id.as_uuid()))
            .bind(filters.product_id.map(|id| id.as_uuid()))
            .bind(filters.result.map(AuditResult::as_str))
            .bind(filters.from)
            .bind(filters.to)
            .bind(after.map(|cursor| cursor.timestamp))
            .bind(after.map(|cursor| cursor.id))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(audit_summary).collect()
    }

    async fn get_audit(&self, id: Uuid) -> AuditSecurityResult<Option<AuditEventDetail>> {
        let sql = format!(
            "{AUDIT_DETAIL_SELECT}
             WHERE e.id = $1"
        );
        let Some(row) = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let summary = audit_summary(&row)?;
        let actor = staff_actor_detail(&row)?;
        let metadata: serde_json::Value = row.try_get("metadata").map_err(storage)?;
        Ok(Some(AuditEventDetail {
            summary,
            actor,
            metadata,
        }))
    }

    async fn list_security(
        &self,
        filters: &SecurityFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<SecurityEventSummary>> {
        let sql = format!(
            "{SECURITY_SELECT}
             WHERE ($1::text IS NULL OR e.kind = $1)
               AND ($2::uuid IS NULL OR e.organization_id = $2)
               AND ($3::uuid IS NULL OR e.installation_id = $3)
               AND ($4::uuid IS NULL OR e.account_id = $4)
               AND ($5::timestamptz IS NULL OR e.created_at >= $5)
               AND ($6::timestamptz IS NULL OR e.created_at <= $6)
               AND ($7::timestamptz IS NULL OR (e.created_at, e.id) < ($7, $8::uuid))
             ORDER BY e.created_at DESC, e.id DESC
             LIMIT $9"
        );
        let rows = sqlx::query(&sql)
            .bind(filters.kind.map(SecurityKind::as_str))
            .bind(filters.organization_id.map(|id| id.as_uuid()))
            .bind(filters.installation_id.map(|id| id.as_uuid()))
            .bind(filters.account_id.map(|id| id.as_uuid()))
            .bind(filters.from)
            .bind(filters.to)
            .bind(after.map(|cursor| cursor.timestamp))
            .bind(after.map(|cursor| cursor.id))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(security_summary).collect()
    }

    async fn get_security(&self, id: Uuid) -> AuditSecurityResult<Option<SecurityEventDetail>> {
        let sql = format!(
            "{SECURITY_DETAIL_SELECT}
             WHERE e.id = $1"
        );
        let Some(row) = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let summary = security_summary(&row)?;
        let account = commercial_account_detail(&row)?;
        let metadata: serde_json::Value = row.try_get("metadata").map_err(storage)?;
        Ok(Some(SecurityEventDetail {
            summary,
            account,
            metadata,
        }))
    }

    async fn list_staff_auth(
        &self,
        filters: &StaffAuthFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<StaffAuthEventSummary>> {
        let sql = format!(
            "{STAFF_AUTH_SELECT}
             WHERE ($1::uuid IS NULL OR e.account_id = $1)
               AND ($2::text IS NULL OR e.action = $2)
               AND ($3::timestamptz IS NULL OR e.occurred_at >= $3)
               AND ($4::timestamptz IS NULL OR e.occurred_at <= $4)
               AND ($5::timestamptz IS NULL OR (e.occurred_at, e.id) < ($5, $6::uuid))
             ORDER BY e.occurred_at DESC, e.id DESC
             LIMIT $7"
        );
        let rows = sqlx::query(&sql)
            .bind(filters.account_id.map(|id| id.as_uuid()))
            .bind(filters.action.map(StaffAuthAction::as_str))
            .bind(filters.from)
            .bind(filters.to)
            .bind(after.map(|cursor| cursor.timestamp))
            .bind(after.map(|cursor| cursor.id))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(staff_auth_summary).collect()
    }

    async fn get_staff_auth(&self, id: Uuid) -> AuditSecurityResult<Option<StaffAuthEventDetail>> {
        let sql = format!(
            "{STAFF_AUTH_DETAIL_SELECT}
             WHERE e.id = $1"
        );
        let Some(row) = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let summary = staff_auth_summary(&row)?;
        let account = staff_actor_detail(&row)?;
        let metadata: serde_json::Value = row.try_get("metadata").map_err(storage)?;
        Ok(Some(StaffAuthEventDetail {
            summary,
            account,
            metadata,
        }))
    }
}
