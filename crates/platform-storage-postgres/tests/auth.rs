use platform_application::{
    auth::{AuthRepository as _, AuthService},
    commercial::CommercialService,
    security::{hash_password, hash_token},
};
use platform_core::{auth::CommercialCredential, commercial::CommercialAccount};
use platform_storage_postgres::{PgAuthRepository, PgCommercialRepository, open};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn isolated_url(name: &str) -> String {
    let url = std::env::var(name).expect("configura una base PostgreSQL aislada de pruebas");
    let database = url.rsplit('/').next().unwrap_or_default();
    assert!(
        database.contains("test") && database != "hiqnet",
        "la base de datos debe ser aislada y nombrarse como test"
    );
    url
}

async fn account_with_password(pool: &PgPool, password: &str) -> (CommercialAccount, String) {
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

fn service(pool: &PgPool) -> AuthService<PgAuthRepository> {
    AuthService::new(PgAuthRepository::new(pool.clone()))
}

#[tokio::test]
async fn login_creates_persistent_hashed_session_and_logout_revokes_it() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let (account, email) = account_with_password(&pool, "contraseña-correcta-larga").await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    let issued = auth
        .login(&email, "contraseña-correcta-larga", now, Duration::hours(1))
        .await
        .unwrap();
    assert!(issued.token.len() >= 40);
    let session = auth.authenticate(&issued.token, now).await.unwrap();
    assert_eq!(session.account_id, account.id);

    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM commercial_session WHERE account_id = $1")
            .bind(account.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, hash_token(&issued.token));
    assert_ne!(stored, issued.token.as_bytes());

    // Persistencia: un pool nuevo, simulando reinicio del servidor, sigue validando.
    let reopened = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    assert!(
        service(&reopened)
            .authenticate(&issued.token, now)
            .await
            .is_ok()
    );

    auth.logout(&issued.token, now).await.unwrap();
    assert!(auth.authenticate(&issued.token, now).await.is_err());
    let revoked: bool = sqlx::query_scalar(
        "SELECT revoked_at IS NOT NULL FROM commercial_session WHERE account_id = $1",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked);
    let logout_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_auth_event WHERE account_id = $1 AND action = 'logout'",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(logout_events, 1);
    assert_eq!(auth.logout(&issued.token, now).await.unwrap(), ());
}

#[tokio::test]
async fn wrong_password_locks_account_and_never_reveals_existence() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let (_account, email) = account_with_password(&pool, "contraseña-correcta-larga").await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    for _ in 0..5 {
        assert!(
            auth.login(&email, "incorrecta", now, Duration::hours(1))
                .await
                .is_err()
        );
    }
    // Incluso con la contraseña correcta, la cuenta queda bloqueada.
    assert!(
        auth.login(&email, "contraseña-correcta-larga", now, Duration::hours(1))
            .await
            .is_err()
    );
    // Cuenta inexistente devuelve el mismo error.
    let unknown = auth
        .login("nadie@example.test", "x", now, Duration::hours(1))
        .await;
    assert!(unknown.is_err());

    let after_lock = now + Duration::minutes(16);
    let issued = auth
        .login(
            &email,
            "contraseña-correcta-larga",
            after_lock,
            Duration::hours(1),
        )
        .await
        .unwrap();
    assert!(auth.authenticate(&issued.token, after_lock).await.is_ok());

    let failures: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_auth_event WHERE action = 'login_failure' AND metadata->>'reason' = 'locked'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(failures >= 1);
}

#[tokio::test]
async fn expired_disabled_and_rotated_sessions_are_rejected() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let (account, email) = account_with_password(&pool, "contraseña-correcta-larga").await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    let first = auth
        .login(&email, "contraseña-correcta-larga", now, Duration::hours(1))
        .await
        .unwrap();
    let second = auth
        .login(&email, "contraseña-correcta-larga", now, Duration::hours(1))
        .await
        .unwrap();
    assert!(
        auth.authenticate(&first.token, now).await.is_err(),
        "rotación invalida la sesión previa"
    );
    assert!(auth.authenticate(&second.token, now).await.is_ok());
    assert!(
        auth.authenticate(&second.token, now + Duration::hours(2))
            .await
            .is_err(),
        "expiración"
    );

    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        auth.authenticate(&second.token, now).await.is_err(),
        "cuenta deshabilitada"
    );
    assert!(
        auth.login(&email, "contraseña-correcta-larga", now, Duration::hours(1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn expired_lock_starts_a_fresh_failure_cycle_without_sleeping() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let (account, _email) = account_with_password(&pool, "contraseña-correcta-larga").await;
    let repo = PgAuthRepository::new(pool.clone());
    let now = OffsetDateTime::now_utc();

    for _ in 0..4 {
        assert!(!repo.register_failure(account.id, now).await.unwrap());
    }
    assert!(!repo.is_locked(account.id, now).await.unwrap());
    assert!(repo.register_failure(account.id, now).await.unwrap());
    assert!(
        repo.is_locked(account.id, now + Duration::minutes(1))
            .await
            .unwrap()
    );

    // Al vencer el bloqueo, el primer fallo inicia un ciclo nuevo (1 intento),
    // no un bloqueo inmediato por arrastrar el contador anterior.
    let after = now + Duration::minutes(16);
    assert!(!repo.is_locked(account.id, after).await.unwrap());
    assert!(!repo.register_failure(account.id, after).await.unwrap());
    for _ in 0..3 {
        assert!(!repo.register_failure(account.id, after).await.unwrap());
    }
    assert!(repo.register_failure(account.id, after).await.unwrap());
    assert!(
        repo.is_locked(account.id, after + Duration::seconds(1))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn tampered_token_and_missing_session_do_not_authenticate() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();
    assert!(auth.authenticate("token-inventado", now).await.is_err());
    assert!(auth.authenticate("", now).await.is_err());
}

#[tokio::test]
async fn migration_0003_preserves_existing_commercial_accounts() {
    let url = isolated_url("TEST_UPGRADE_DATABASE_URL");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    let migrations_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations')::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    let already: bool = if migrations_table.is_some() {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = 3 AND success)",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    } else {
        false
    };
    let email = "migracion-persistente@example.test".to_owned();
    if !already {
        let mut previous = sqlx::migrate!("../../migrations");
        previous.migrations = std::borrow::Cow::Owned(
            previous
                .iter()
                .filter(|migration| migration.version <= 2)
                .cloned()
                .collect(),
        );
        previous.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO commercial_account (id, email, display_name, status, created_at)
             VALUES ($1, $2, 'Cuenta Previa', 'active', now())",
        )
        .bind(Uuid::new_v4())
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();
    }
    drop(pool);
    let upgraded = open(&url).await.unwrap();
    let preserved: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM commercial_account WHERE email = $1)")
            .bind(&email)
            .fetch_one(&upgraded)
            .await
            .unwrap();
    assert!(preserved, "la cuenta previa a 0003 se conserva");
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'
         AND table_name IN ('commercial_credential','commercial_session','commercial_auth_event')",
    )
    .fetch_one(&upgraded)
    .await
    .unwrap();
    assert_eq!(tables, 3);
    open(&url).await.expect("migraciones idempotentes");
    let _ = upgraded;
}
