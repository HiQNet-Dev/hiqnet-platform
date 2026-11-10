//! Adaptador PostgreSQL de mutaciones OrganizationProduct.

use async_trait::async_trait;
use platform_application::{
    control::{AssociationStatus, ControlOrganizationProductView},
    organization_product::{
        OrganizationProductError, OrganizationProductMutation, OrganizationProductRepository,
        OrganizationProductResult,
    },
};
use platform_core::{
    ids::{OrganizationId, ProductId, StaffAccountId},
    product::LifecycleStatus,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgOrganizationProductRepository {
    pool: PgPool,
}

impl PgOrganizationProductRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> OrganizationProductError {
    OrganizationProductError::Storage
}

async fn exists(
    tx: &mut Transaction<'_, Postgres>,
    table: &'static str,
    id: Uuid,
) -> OrganizationProductResult<bool> {
    let sql = match table {
        "organization" => "SELECT EXISTS(SELECT 1 FROM organization WHERE id = $1)",
        "product" => "SELECT EXISTS(SELECT 1 FROM product WHERE id = $1)",
        _ => return Err(OrganizationProductError::Storage),
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(storage)
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: StaffAccountId,
    organization: OrganizationId,
    product: ProductId,
    action: &str,
    now: OffsetDateTime,
) -> OrganizationProductResult<()> {
    sqlx::query(
        "INSERT INTO staff_audit_event
         (id, staff_account_id, action, resource_type, resource_id, organization_id, product_id,
          result, occurred_at, metadata)
         VALUES ($1, $2, $3, 'organization_product', $4, $5, $4, 'success', $6, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(actor.as_uuid())
    .bind(action)
    .bind(product.as_uuid())
    .bind(organization.as_uuid())
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

async fn view(
    tx: &mut Transaction<'_, Postgres>,
    organization: OrganizationId,
    product: ProductId,
) -> OrganizationProductResult<ControlOrganizationProductView> {
    let row = sqlx::query(
        "SELECT op.product_id, p.key, p.name, p.status AS product_status,
                op.status AS association_status, op.activated_at, op.entitlements
         FROM organization_product op
         JOIN product p ON p.id = op.product_id
         WHERE op.organization_id = $1 AND op.product_id = $2",
    )
    .bind(organization.as_uuid())
    .bind(product.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?
    .ok_or(OrganizationProductError::Storage)?;
    let product_status: String = row.try_get("product_status").map_err(storage)?;
    let association_status: String = row.try_get("association_status").map_err(storage)?;
    Ok(ControlOrganizationProductView {
        product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage)?),
        product_key: row.try_get("key").map_err(storage)?,
        product_name: row.try_get("name").map_err(storage)?,
        product_status: LifecycleStatus::from_str(&product_status)
            .map_err(|_| OrganizationProductError::Storage)?,
        association_status: AssociationStatus::from_str(&association_status)
            .map_err(|_| OrganizationProductError::Storage)?,
        activated_at: row.try_get("activated_at").map_err(storage)?,
        entitlements: row.try_get("entitlements").map_err(storage)?,
    })
}

#[async_trait]
impl OrganizationProductRepository for PgOrganizationProductRepository {
    async fn list(
        &self,
        organization_id: OrganizationId,
    ) -> OrganizationProductResult<Vec<ControlOrganizationProductView>> {
        let organization_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization WHERE id = $1)")
                .bind(organization_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(storage)?;
        if !organization_exists {
            return Err(OrganizationProductError::NotFound);
        }
        let rows = sqlx::query(
            "SELECT op.product_id, p.key, p.name, p.status AS product_status,
                    op.status AS association_status, op.activated_at, op.entitlements
             FROM organization_product op
             JOIN product p ON p.id = op.product_id
             WHERE op.organization_id = $1
             ORDER BY p.name, p.key",
        )
        .bind(organization_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let product_status: String = row.try_get("product_status").map_err(storage)?;
                let association_status: String =
                    row.try_get("association_status").map_err(storage)?;
                Ok(ControlOrganizationProductView {
                    product_id: ProductId::from_uuid(row.try_get("product_id").map_err(storage)?),
                    product_key: row.try_get("key").map_err(storage)?,
                    product_name: row.try_get("name").map_err(storage)?,
                    product_status: LifecycleStatus::from_str(&product_status)
                        .map_err(|_| OrganizationProductError::Storage)?,
                    association_status: AssociationStatus::from_str(&association_status)
                        .map_err(|_| OrganizationProductError::Storage)?,
                    activated_at: row.try_get("activated_at").map_err(storage)?,
                    entitlements: row.try_get("entitlements").map_err(storage)?,
                })
            })
            .collect()
    }

    async fn activate(
        &self,
        actor: StaffAccountId,
        organization_id: OrganizationId,
        product_id: ProductId,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if !exists(&mut tx, "organization", organization_id.as_uuid()).await?
            || !exists(&mut tx, "product", product_id.as_uuid()).await?
        {
            tx.rollback().await.map_err(storage)?;
            return Err(OrganizationProductError::NotFound);
        }
        let inserted = sqlx::query(
            "INSERT INTO organization_product
             (organization_id, product_id, status, activated_at)
             VALUES ($1, $2, 'active', $3)
             ON CONFLICT (organization_id, product_id) DO NOTHING",
        )
        .bind(organization_id.as_uuid())
        .bind(product_id.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected()
            == 1;
        let current: String = sqlx::query_scalar(
            "SELECT status FROM organization_product
             WHERE organization_id = $1 AND product_id = $2 FOR UPDATE",
        )
        .bind(organization_id.as_uuid())
        .bind(product_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        let mut changed = inserted;
        if !inserted && current == "inactive" {
            sqlx::query(
                "UPDATE organization_product SET status = 'active'
                 WHERE organization_id = $1 AND product_id = $2",
            )
            .bind(organization_id.as_uuid())
            .bind(product_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            changed = true;
        }
        if changed {
            audit(
                &mut tx,
                actor,
                organization_id,
                product_id,
                "organization_product_activated",
                now,
            )
            .await?;
        }
        let view = view(&mut tx, organization_id, product_id).await?;
        tx.commit().await.map_err(storage)?;
        Ok(OrganizationProductMutation { view, changed })
    }

    async fn set_status(
        &self,
        actor: StaffAccountId,
        organization_id: OrganizationId,
        product_id: ProductId,
        status: AssociationStatus,
        now: OffsetDateTime,
    ) -> OrganizationProductResult<OrganizationProductMutation> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let current: Option<String> = sqlx::query_scalar(
            "SELECT status FROM organization_product
             WHERE organization_id = $1 AND product_id = $2 FOR UPDATE",
        )
        .bind(organization_id.as_uuid())
        .bind(product_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(current) = current else {
            tx.rollback().await.map_err(storage)?;
            return Err(OrganizationProductError::NotFound);
        };
        let target = status.as_str();
        let changed = current != target;
        if changed {
            sqlx::query(
                "UPDATE organization_product SET status = $3
                 WHERE organization_id = $1 AND product_id = $2",
            )
            .bind(organization_id.as_uuid())
            .bind(product_id.as_uuid())
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            let action = if target == "active" {
                "organization_product_activated"
            } else {
                "organization_product_deactivated"
            };
            audit(&mut tx, actor, organization_id, product_id, action, now).await?;
        }
        let view = view(&mut tx, organization_id, product_id).await?;
        tx.commit().await.map_err(storage)?;
        Ok(OrganizationProductMutation { view, changed })
    }
}
