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
