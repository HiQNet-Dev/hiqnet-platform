//! Adaptador PostgreSQL: conexión, migraciones y repositorio.

use sqlx::PgPool;

pub mod repository;

pub use repository::PgRepository;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("no fue posible abrir la base de datos")]
    Open,

    #[error("no fue posible aplicar las migraciones")]
    Migrate,
}

pub async fn open(database_url: &str) -> Result<PgPool, StorageError> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|_| StorageError::Open)?;
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .map_err(|_| StorageError::Migrate)?;
    Ok(pool)
}

pub fn pool_repository(pool: PgPool) -> PgRepository {
    PgRepository::new(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn applies_initial_migration_and_rolls_back_synthetic_data() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL debe apuntar a una base aislada de pruebas");
        let pool = open(&url).await.expect("migración inicial");
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_name IN ('installation', 'issued_license')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tables, 2);

        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO installation (id, organization_id, device_label, status, registered_at) \
             VALUES ($1, $2, 'fixture-p00', 'active', now())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(uuid::Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM installation WHERE device_label = 'fixture-p00'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
    }
}
