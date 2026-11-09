//! Adaptador PostgreSQL: conexión, migraciones y repositorio.

use sqlx::PgPool;

pub mod auth;
pub mod commercial;
pub mod control;
pub mod enrollment;
pub mod installation_control;
pub mod product;
pub mod repository;
pub mod staff;

pub use auth::PgAuthRepository;
pub use commercial::PgCommercialRepository;
pub use control::PgControlRepository;
pub use enrollment::PgEnrollmentRepository;
pub use installation_control::PgInstallationControlRepository;
pub use product::PgProductRepository;
pub use repository::PgRepository;
pub use staff::PgStaffRepository;

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

/// Un fallo en el roundtrip indica indisponibilidad; no se filtran errores de conexión al cliente.
pub async fn is_ready(pool: &PgPool) -> bool {
    matches!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await,
        Ok(1)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn applies_initial_migration_and_rolls_back_synthetic_data() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL debe apuntar a una base aislada de pruebas");
        assert!(
            url.rsplit('/')
                .next()
                .is_some_and(|database| database.contains("test") && database != "hiqnet"),
            "TEST_DATABASE_URL debe identificar una base aislada llamada test"
        );
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
        let installation_id = uuid::Uuid::new_v4();
        let organization_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'fixture-p00', 'active', now())",
        )
        .bind(organization_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO installation (id, organization_id, device_label, status, registered_at) \
             VALUES ($1, $2, 'fixture-p00', 'active', now())",
        )
        .bind(installation_id)
        .bind(organization_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        let grant_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO issued_license (grant_id, installation_id, organization_id, plan, \
             features_json, issued_at, valid_until, grant_json, signature_b64) \
             VALUES ($1, $2, $3, 'trial', '{}'::jsonb, now(), now() + interval '1 day', '{}', 'synthetic')",
        )
        .bind(grant_id)
        .bind(installation_id)
        .bind(organization_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        let plan: String =
            sqlx::query_scalar("SELECT plan FROM issued_license WHERE grant_id = $1")
                .bind(grant_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(plan, "trial");
        sqlx::query("UPDATE installation SET status = 'suspended' WHERE id = $1")
            .bind(installation_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM installation WHERE id = $1")
            .bind(installation_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(status, "suspended");
        sqlx::query("DELETE FROM issued_license WHERE grant_id = $1")
            .bind(grant_id)
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
