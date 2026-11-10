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

pub(crate) async fn account_with_password(
    pool: &PgPool,
    password: &str,
) -> (CommercialAccount, String) {
    let now = OffsetDateTime::now_utc();
    let email = format!("auth-{}@example.test", Uuid::new_v4());
    let account = CommercialService::new(PgCommercialRepository::new(pool.clone()))
        .create_account(&email, "Cuenta Auth", now)
        .await
        .unwrap();
    let hash = hash_password(password.to_owned()).await.unwrap();
    PgAuthRepository::new(pool.clone())
        .create_credential(&CommercialCredential::new(account.id, hash, now))
        .await
        .unwrap();
    (account, email)
}

pub(crate) fn service(pool: &PgPool) -> AuthService<PgAuthRepository> {
    AuthService::new(PgAuthRepository::new(pool.clone()))
}

pub(crate) async fn session_id(
    auth: &AuthService<PgAuthRepository>,
    token: &str,
    now: OffsetDateTime,
) -> uuid::Uuid {
    auth.authenticate(token, now).await.unwrap().session_id
}
