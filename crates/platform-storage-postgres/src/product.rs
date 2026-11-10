//! Persistencia del registro de productos y proyectos, con auditoría atómica.

use async_trait::async_trait;
use platform_application::product::{ProductRepository, StaffActor};
use platform_core::{
    ids::{ProductId, ProjectId},
    product::{
        LifecycleStatus, NewProduct, NewProject, Product, ProductChanges, ProductError,
        ProductResult, ProductType, Project, ProjectChanges, ProjectType,
    },
};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use uuid::Uuid;

pub struct PgProductRepository {
    pool: PgPool,
}

impl PgProductRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage(_error: sqlx::Error) -> ProductError {
    ProductError::Storage
}

fn write_error(error: sqlx::Error) -> ProductError {
    if let sqlx::Error::Database(database) = &error {
        if let Some(code) = database.code().as_deref() {
            return match code {
                "23505" => ProductError::Conflict,
                "23503" => ProductError::InvalidInput,
                _ => ProductError::Storage,
            };
        }
    }
    ProductError::Storage
}

fn status_from_row(row: &sqlx::postgres::PgRow) -> ProductResult<LifecycleStatus> {
    let value: String = row.get("status");
    value.parse().map_err(|_| ProductError::Storage)
}

fn product_type_from_row(row: &sqlx::postgres::PgRow) -> ProductResult<ProductType> {
    let value: String = row.get("product_type");
    value.parse().map_err(|_| ProductError::Storage)
}

fn project_type_from_row(row: &sqlx::postgres::PgRow) -> ProductResult<ProjectType> {
    let value: String = row.get("project_type");
    value.parse().map_err(|_| ProductError::Storage)
}

fn product_from_row(row: &sqlx::postgres::PgRow) -> ProductResult<Product> {
    Ok(Product {
        id: ProductId::from_uuid(row.get("id")),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        status: status_from_row(row)?,
        product_type: product_type_from_row(row)?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn project_from_row(row: &sqlx::postgres::PgRow) -> ProductResult<Project> {
    let product_id: Option<Uuid> = row.get("product_id");
    Ok(Project {
        id: ProjectId::from_uuid(row.get("id")),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        project_type: project_type_from_row(row)?,
        status: status_from_row(row)?,
        product_id: product_id.map(ProductId::from_uuid),
        product_key: row.get("product_key"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    actor: &StaffActor,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    product_id: Option<Uuid>,
    now: OffsetDateTime,
) -> ProductResult<()> {
    sqlx::query(
        "INSERT INTO staff_audit_event
         (id, staff_account_id, action, resource_type, resource_id, product_id, result, occurred_at, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, 'success', $7, '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(actor.account_id.as_uuid())
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(product_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

#[async_trait]
impl ProductRepository for PgProductRepository {
    async fn list_products(
        &self,
        status: Option<LifecycleStatus>,
        product_type: Option<ProductType>,
        limit: u32,
        after: Option<&str>,
    ) -> ProductResult<Vec<Product>> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT id, key, name, description, status, product_type, created_at, updated_at
             FROM product WHERE 1 = 1",
        );
        if let Some(status) = status {
            query.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(product_type) = product_type {
            query
                .push(" AND product_type = ")
                .push_bind(product_type.as_str());
        }
        if let Some(after) = after {
            query.push(" AND key > ").push_bind(after);
        }
        query
            .push(" ORDER BY key ASC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(&self.pool).await.map_err(storage)?;
        rows.iter().map(product_from_row).collect()
    }

    async fn get_product(&self, key: &str) -> ProductResult<Option<Product>> {
        let row = sqlx::query(
            "SELECT id, key, name, description, status, product_type, created_at, updated_at
             FROM product WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(product_from_row).transpose()
    }

    async fn create_product(
        &self,
        actor: &StaffActor,
        product: &NewProduct,
        now: OffsetDateTime,
    ) -> ProductResult<Product> {
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query(
            "INSERT INTO product (id, key, name, description, status, product_type, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
        )
        .bind(id)
        .bind(&product.key)
        .bind(product.name.trim())
        .bind(&product.description)
        .bind(product.status.as_str())
        .bind(product.product_type.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        insert_audit(
            &mut tx,
            actor,
            "product_created",
            "product",
            id,
            Some(id),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        self.get_product(&product.key)
            .await?
            .ok_or(ProductError::Storage)
    }

    async fn update_product(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: &ProductChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Product> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "UPDATE product SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                updated_at = $5
             WHERE key = $1
             RETURNING id, key, name, description, status, product_type, created_at, updated_at",
        )
        .bind(key)
        .bind(changes.name.as_deref().map(str::trim))
        .bind(changes.description.as_deref())
        .bind(changes.status.map(|status| status.as_str()))
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_error)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Err(ProductError::NotFound);
        };
        let product = product_from_row(&row)?;
        insert_audit(
            &mut tx,
            actor,
            "product_updated",
            "product",
            product.id.as_uuid(),
            Some(product.id.as_uuid()),
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        Ok(product)
    }

    async fn list_projects(
        &self,
        product_key: Option<&str>,
        limit: u32,
        after: Option<&str>,
    ) -> ProductResult<Vec<Project>> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT p.id, p.key, p.name, p.description, p.project_type, p.status, p.product_id,
                    pr.key AS product_key, p.created_at, p.updated_at
             FROM project p LEFT JOIN product pr ON pr.id = p.product_id WHERE 1 = 1",
        );
        if let Some(product_key) = product_key {
            query
                .push(" AND p.product_id = (SELECT id FROM product WHERE key = ")
                .push_bind(product_key)
                .push(")");
        }
        if let Some(after) = after {
            query.push(" AND p.key > ").push_bind(after);
        }
        query
            .push(" ORDER BY p.key ASC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(&self.pool).await.map_err(storage)?;
        rows.iter().map(project_from_row).collect()
    }

    async fn get_project(&self, key: &str) -> ProductResult<Option<Project>> {
        let row = sqlx::query(
            "SELECT p.id, p.key, p.name, p.description, p.project_type, p.status, p.product_id,
                    pr.key AS product_key, p.created_at, p.updated_at
             FROM project p LEFT JOIN product pr ON pr.id = p.product_id
             WHERE p.key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(project_from_row).transpose()
    }

    async fn create_project(
        &self,
        actor: &StaffActor,
        project: &NewProject,
        now: OffsetDateTime,
    ) -> ProductResult<Project> {
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let product_id: Option<Uuid> = match project.product_key.as_deref() {
            Some(key) => {
                let found: Option<Uuid> =
                    sqlx::query_scalar("SELECT id FROM product WHERE key = $1")
                        .bind(key)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(storage)?;
                Some(found.ok_or(ProductError::InvalidInput)?)
            }
            None => None,
        };
        sqlx::query(
            "INSERT INTO project
             (id, key, name, description, project_type, status, product_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        )
        .bind(id)
        .bind(&project.key)
        .bind(project.name.trim())
        .bind(&project.description)
        .bind(project.project_type.as_str())
        .bind(project.status.as_str())
        .bind(product_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        insert_audit(
            &mut tx,
            actor,
            "project_created",
            "project",
            id,
            product_id,
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        self.get_project(&project.key)
            .await?
            .ok_or(ProductError::Storage)
    }

    async fn update_project(
        &self,
        actor: &StaffActor,
        key: &str,
        changes: &ProjectChanges,
        now: OffsetDateTime,
    ) -> ProductResult<Project> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "UPDATE project SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                updated_at = $5
             WHERE key = $1
             RETURNING id, key, name, description, project_type, status, product_id,
                       created_at, updated_at",
        )
        .bind(key)
        .bind(changes.name.as_deref().map(str::trim))
        .bind(changes.description.as_deref())
        .bind(changes.status.map(|status| status.as_str()))
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(write_error)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Err(ProductError::NotFound);
        };
        let id: Uuid = row.get("id");
        let product_id: Option<Uuid> = row.get("product_id");
        insert_audit(
            &mut tx,
            actor,
            "project_updated",
            "project",
            id,
            product_id,
            now,
        )
        .await?;
        tx.commit().await.map_err(storage)?;
        self.get_project(key).await?.ok_or(ProductError::Storage)
    }
}
