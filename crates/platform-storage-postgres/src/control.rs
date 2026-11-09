//! Proyecciones globales read-only del Control Center.
//!
//! Consulta las mismas tablas que la superficie comercial, pero sin reutilizar
//! su repositorio ni su autoridad. No modifica datos ni audita lecturas.

use async_trait::async_trait;
use platform_application::control::{
    AccountStatus, AssociationStatus, ControlAccountDetail, ControlAccountMembershipView,
    ControlAccountSummary, ControlCursor, ControlError, ControlMembershipView,
    ControlOrganizationDetail, ControlOrganizationProductView, ControlOrganizationSummary,
    ControlRepository, ControlResult, MembershipRole, OrganizationStatus,
};
use platform_core::{
    ids::{CommercialAccountId, OrganizationId, ProductId},
    product::LifecycleStatus,
};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

pub struct PgControlRepository {
    pool: PgPool,
}

impl PgControlRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> ControlError {
    ControlError::Storage
}

fn organization_status(row: &sqlx::postgres::PgRow) -> ControlResult<OrganizationStatus> {
    let value: String = row.get("status");
    value.parse()
}

fn account_status(row: &sqlx::postgres::PgRow, column: &str) -> ControlResult<AccountStatus> {
    let value: String = row.get(column);
    value.parse()
}

fn role(row: &sqlx::postgres::PgRow) -> ControlResult<MembershipRole> {
    let value: String = row.get("role");
    value.parse()
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn summary_from_row(row: &sqlx::postgres::PgRow) -> ControlResult<ControlOrganizationSummary> {
    Ok(ControlOrganizationSummary {
        id: OrganizationId::from_uuid(row.get("id")),
        display_name: row.get("display_name"),
        status: organization_status(row)?,
        created_at: row.get("created_at"),
    })
}

fn account_summary_from_row(row: &sqlx::postgres::PgRow) -> ControlResult<ControlAccountSummary> {
    Ok(ControlAccountSummary {
        id: CommercialAccountId::from_uuid(row.get("id")),
        email: row.get("email"),
        display_name: row.get("display_name"),
        status: account_status(row, "status")?,
        created_at: row.get("created_at"),
    })
}

#[async_trait]
impl ControlRepository for PgControlRepository {
    async fn list_organizations(
        &self,
        status: Option<OrganizationStatus>,
        search: Option<&str>,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> ControlResult<Vec<ControlOrganizationSummary>> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT id, display_name, status, created_at FROM organization WHERE 1 = 1",
        );
        if let Some(status) = status {
            query.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(search) = search {
            query
                .push(" AND display_name ILIKE ")
                .push_bind(format!("%{}%", escape_like(search)))
                .push(" ESCAPE '\\'");
        }
        if let Some(cursor) = after {
            query
                .push(" AND (created_at, id) > (")
                .push_bind(cursor.created_at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        query
            .push(" ORDER BY created_at ASC, id ASC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(&self.pool).await.map_err(storage)?;
        rows.iter().map(summary_from_row).collect()
    }

    async fn get_organization(
        &self,
        id: OrganizationId,
    ) -> ControlResult<Option<ControlOrganizationDetail>> {
        let row = sqlx::query(
            "SELECT id, display_name, status, created_at FROM organization WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let organization = summary_from_row(&row)?;

        let membership_rows = sqlx::query(
            "SELECT m.account_id, a.email, a.display_name, a.status AS account_status,
                    m.role, m.joined_at, m.revoked_at
             FROM organization_membership m
             JOIN commercial_account a ON a.id = m.account_id
             WHERE m.organization_id = $1
             ORDER BY m.joined_at ASC, m.account_id ASC",
        )
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let memberships = membership_rows
            .iter()
            .map(|row| {
                Ok(ControlMembershipView {
                    account_id: CommercialAccountId::from_uuid(row.get("account_id")),
                    email: row.get("email"),
                    display_name: row.get("display_name"),
                    account_status: account_status(row, "account_status")?,
                    role: role(row)?,
                    joined_at: row.get("joined_at"),
                    revoked_at: row.get("revoked_at"),
                })
            })
            .collect::<ControlResult<Vec<_>>>()?;

        let product_rows = sqlx::query(
            "SELECT op.product_id, p.key AS product_key, p.name AS product_name,
                    p.status AS product_status, op.status AS association_status,
                    op.activated_at, op.entitlements
             FROM organization_product op
             JOIN product p ON p.id = op.product_id
             WHERE op.organization_id = $1
             ORDER BY op.activated_at ASC, op.product_id ASC",
        )
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let products = product_rows
            .iter()
            .map(|row| {
                let product_status: String = row.get("product_status");
                let association_status: String = row.get("association_status");
                Ok(ControlOrganizationProductView {
                    product_id: ProductId::from_uuid(row.get("product_id")),
                    product_key: row.get("product_key"),
                    product_name: row.get("product_name"),
                    product_status: product_status
                        .parse::<LifecycleStatus>()
                        .map_err(|_| ControlError::Storage)?,
                    association_status: association_status.parse::<AssociationStatus>()?,
                    activated_at: row.get("activated_at"),
                    entitlements: row.get("entitlements"),
                })
            })
            .collect::<ControlResult<Vec<_>>>()?;

        Ok(Some(ControlOrganizationDetail {
            organization,
            memberships,
            products,
        }))
    }

    async fn list_accounts(
        &self,
        status: Option<AccountStatus>,
        search: Option<&str>,
        limit: u32,
        after: Option<ControlCursor>,
    ) -> ControlResult<Vec<ControlAccountSummary>> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT id, email, display_name, status, created_at
             FROM commercial_account WHERE 1 = 1",
        );
        if let Some(status) = status {
            query.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(search) = search {
            let pattern = format!("%{}%", escape_like(search));
            query
                .push(" AND (email ILIKE ")
                .push_bind(pattern.clone())
                .push(" ESCAPE '\\' OR display_name ILIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\')");
        }
        if let Some(cursor) = after {
            query
                .push(" AND (created_at, id) > (")
                .push_bind(cursor.created_at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        query
            .push(" ORDER BY created_at ASC, id ASC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(&self.pool).await.map_err(storage)?;
        rows.iter().map(account_summary_from_row).collect()
    }

    async fn get_account(
        &self,
        id: CommercialAccountId,
    ) -> ControlResult<Option<ControlAccountDetail>> {
        let row = sqlx::query(
            "SELECT id, email, display_name, status, created_at
             FROM commercial_account WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let account = account_summary_from_row(&row)?;

        let membership_rows = sqlx::query(
            "SELECT m.organization_id, o.display_name AS organization_display_name,
                    o.status AS organization_status, m.role, m.joined_at, m.revoked_at
             FROM organization_membership m
             JOIN organization o ON o.id = m.organization_id
             WHERE m.account_id = $1
             ORDER BY m.joined_at ASC, m.organization_id ASC",
        )
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let memberships = membership_rows
            .iter()
            .map(|row| {
                let organization_status: String = row.get("organization_status");
                Ok(ControlAccountMembershipView {
                    organization_id: OrganizationId::from_uuid(row.get("organization_id")),
                    organization_display_name: row.get("organization_display_name"),
                    organization_status: organization_status.parse()?,
                    role: role(row)?,
                    joined_at: row.get("joined_at"),
                    revoked_at: row.get("revoked_at"),
                })
            })
            .collect::<ControlResult<Vec<_>>>()?;

        Ok(Some(ControlAccountDetail {
            account,
            memberships,
        }))
    }
}
