//! Fixtures compartidos.

use super::*;

pub(crate) fn isolated_url(name: &str) -> String {
    let url = std::env::var(name).expect("configura una base PostgreSQL aislada de pruebas");
    let database = url.rsplit('/').next().unwrap_or_default();
    assert!(
        database.contains("test") && database != "hiqnet",
        "la base de datos debe ser aislada y nombrarse como test"
    );
    url
}

pub(crate) async fn installation(pool: &PgPool, organization: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installation (id, organization_id, device_label, status, registered_at)
         VALUES ($1, $2, $3, 'active', now())",
    )
    .bind(id)
    .bind(organization)
    .bind(format!("fixture-p021-{id}"))
    .execute(pool)
    .await
    .unwrap();
    id
}
