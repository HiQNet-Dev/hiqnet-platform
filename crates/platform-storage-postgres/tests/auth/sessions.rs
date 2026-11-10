//! Escenario: sessions.

use super::fixtures::*;
use super::*;

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
async fn tampered_token_and_missing_session_do_not_authenticate() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();
    assert!(auth.authenticate("token-inventado", now).await.is_err());
    assert!(auth.authenticate("", now).await.is_err());
}

#[tokio::test]
async fn session_revoked_between_authentication_and_mutation_cannot_change_password() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();
    let session = auth
        .login(&email, initial, now, Duration::hours(1))
        .await
        .unwrap();
    // Estado que había extraído AuthContext antes de la revocación.
    let stale_session_id = session_id(&auth, &session.token, now).await;

    auth.revoke_all_sessions(account.id, now + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(
        auth.change_password(
            account.id,
            stale_session_id,
            initial,
            "nueva-clave-segura-2026",
            now + Duration::seconds(2),
            Duration::hours(1),
        )
        .await,
        Err(platform_core::auth::AuthError::InvalidSession)
    );

    let credential = PgAuthRepository::new(pool.clone())
        .get_credential(account.id)
        .await
        .unwrap()
        .unwrap();
    assert!(platform_application::security::verify_password_sync(
        initial,
        &credential.password_hash
    ));
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_session
         WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > $2",
    )
    .bind(account.id.as_uuid())
    .bind(now + Duration::seconds(2))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 0, "una sesión revocada no debe crear otra");
}

#[tokio::test]
async fn concurrent_login_and_revoke_leave_at_most_one_valid_session() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    let (login, revoke) = tokio::join!(
        auth.login(
            &email,
            initial,
            now + Duration::seconds(1),
            Duration::hours(1)
        ),
        auth.revoke_all_sessions(account.id, now + Duration::seconds(2))
    );
    let issued = login.expect("el login debe completarse");
    revoke.expect("la revocación debe completarse");

    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_session
         WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > $2",
    )
    .bind(account.id.as_uuid())
    .bind(now + Duration::seconds(3))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(active <= 1, "no puede quedar más de una sesión activa");
    let valid = auth
        .authenticate(&issued.token, now + Duration::seconds(3))
        .await
        .is_ok();
    assert_eq!(valid, active == 1);
}

#[tokio::test]
async fn concurrent_logins_preserve_single_session_policy() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    let (one, two) = tokio::join!(
        auth.login(
            &email,
            initial,
            now + Duration::seconds(1),
            Duration::hours(1)
        ),
        auth.login(
            &email,
            initial,
            now + Duration::seconds(2),
            Duration::hours(1)
        )
    );
    let one = one.unwrap();
    let two = two.unwrap();
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_session
         WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > $2",
    )
    .bind(account.id.as_uuid())
    .bind(now + Duration::seconds(3))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 1, "login concurrente debe dejar una sola sesión");
    let one_valid = auth
        .authenticate(&one.token, now + Duration::seconds(3))
        .await
        .is_ok();
    let two_valid = auth
        .authenticate(&two.token, now + Duration::seconds(3))
        .await
        .is_ok();
    assert!(
        one_valid ^ two_valid,
        "sólo la última sesión debe ser válida"
    );
}

#[tokio::test]
async fn login_verified_before_password_change_cannot_create_session() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let new_password = "nueva-clave-segura-2026";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();

    // Hash que un login habría verificado con Argon2id antes del cambio.
    let repository = PgAuthRepository::new(pool.clone());
    let verified_hash = repository
        .get_credential(account.id)
        .await
        .unwrap()
        .unwrap()
        .password_hash;

    let session = auth
        .login(&email, initial, now, Duration::hours(1))
        .await
        .unwrap();
    let current_session_id = session_id(&auth, &session.token, now).await;
    auth.change_password(
        account.id,
        current_session_id,
        initial,
        new_password,
        now + Duration::seconds(1),
        Duration::hours(1),
    )
    .await
    .unwrap();

    let success_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_auth_event WHERE account_id = $1 AND action = 'login_success'",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();

    let (token, token_hash) = generate_session_token();
    let candidate = AuthenticatedSession {
        session_id: Uuid::new_v4(),
        account_id: account.id,
        created_at: now + Duration::seconds(2),
        expires_at: now + Duration::seconds(2) + Duration::hours(1),
    };
    let outcome = repository
        .rotate_session(&verified_hash, &candidate, &token_hash)
        .await
        .unwrap();
    assert_eq!(outcome, LoginOutcome::CredentialChanged);
    assert!(
        auth.authenticate(&token, now + Duration::seconds(2))
            .await
            .is_err(),
        "no debe existir sesión creada con el hash anterior"
    );
    let success_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_auth_event WHERE account_id = $1 AND action = 'login_success'",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(success_before, success_after, "sin login_success parcial");

    assert!(
        auth.login(
            &email,
            new_password,
            now + Duration::seconds(3),
            Duration::hours(1)
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn login_revalidates_disabled_account_and_lock_inside_transaction() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let repository = PgAuthRepository::new(pool.clone());
    let now = OffsetDateTime::now_utc();
    let verified_hash = repository
        .get_credential(account.id)
        .await
        .unwrap()
        .unwrap()
        .password_hash;

    sqlx::query("UPDATE commercial_credential SET locked_until = $2 WHERE account_id = $1")
        .bind(account.id.as_uuid())
        .bind(now + Duration::minutes(10))
        .execute(&pool)
        .await
        .unwrap();
    let (_, token_hash) = generate_session_token();
    let candidate = AuthenticatedSession {
        session_id: Uuid::new_v4(),
        account_id: account.id,
        created_at: now,
        expires_at: now + Duration::hours(1),
    };
    assert_eq!(
        repository
            .rotate_session(&verified_hash, &candidate, &token_hash)
            .await
            .unwrap(),
        LoginOutcome::Locked
    );

    sqlx::query("UPDATE commercial_credential SET locked_until = NULL WHERE account_id = $1")
        .bind(account.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let (_, token_hash) = generate_session_token();
    assert_eq!(
        repository
            .rotate_session(&verified_hash, &candidate, &token_hash)
            .await
            .unwrap(),
        LoginOutcome::AccountUnavailable
    );
    assert!(
        auth.login(&email, initial, now, Duration::hours(1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rotation_uses_database_clock_after_waiting_for_lock() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();
    let first = auth
        .login(&email, initial, now, Duration::hours(1))
        .await
        .unwrap();
    let repository = PgAuthRepository::new(pool.clone());
    let hash = repository
        .get_credential(account.id)
        .await
        .unwrap()
        .unwrap()
        .password_hash;

    // Simula un timestamp capturado antes de esperar el bloqueo: es anterior a
    // la creación de la sesión vigente. El repositorio debe usar now() de
    // PostgreSQL para revocar y crear, no este valor obsoleto.
    let stale_created = now - Duration::hours(1);
    let (token, token_hash) = generate_session_token();
    let candidate = AuthenticatedSession {
        session_id: Uuid::new_v4(),
        account_id: account.id,
        created_at: stale_created,
        expires_at: stale_created + Duration::hours(2),
    };
    let outcome = repository
        .rotate_session(&hash, &candidate, &token_hash)
        .await
        .unwrap();
    assert!(matches!(outcome, LoginOutcome::Rotated { .. }));
    assert!(auth.authenticate(&first.token, now).await.is_err());
    assert!(auth.authenticate(&token, now).await.is_ok());
}

#[tokio::test]
async fn contention_uses_clock_timestamp_after_waiting_for_account_lock() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let initial = "contraseña-inicial-1";
    let (account, email) = account_with_password(&pool, initial).await;
    let auth = service(&pool);
    let now = OffsetDateTime::now_utc();
    let existing = auth
        .login(&email, initial, now, Duration::hours(1))
        .await
        .unwrap();
    // Conexión A: mantiene el bloqueo de la cuenta.
    let mut conn_a = pool.acquire().await.unwrap();
    let pid_a: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *conn_a)
        .await
        .unwrap();
    sqlx::query("BEGIN").execute(&mut *conn_a).await.unwrap();
    sqlx::query("SELECT id FROM commercial_account WHERE id = $1 FOR UPDATE")
        .bind(account.id.as_uuid())
        .execute(&mut *conn_a)
        .await
        .unwrap();

    // Conexión B: inicia su transacción y queda esperando el bloqueo de A.
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<i32>();
    let pool_b = pool.clone();
    let account_uuid = account.id.as_uuid();
    let b_task = tokio::spawn(async move {
        let mut tx = pool_b.begin().await.unwrap();
        let pid_b: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let _ = started_tx.send(pid_b);
        // Se bloquea aquí hasta que A confirme.
        sqlx::query("SELECT id FROM commercial_account WHERE id = $1 FOR UPDATE")
            .bind(account_uuid)
            .execute(&mut *tx)
            .await
            .unwrap();
        let tx_timestamp: OffsetDateTime = sqlx::query_scalar("SELECT transaction_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let clock_timestamp: OffsetDateTime = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let session_id = Uuid::new_v4();
        sqlx::query(
            "UPDATE commercial_session SET revoked_at = clock_timestamp()
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_uuid)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commercial_session
             (id, account_id, token_hash, created_at, expires_at, last_seen_at)
             VALUES ($1, $2, $3, clock_timestamp(),
                     clock_timestamp() + interval '1 hour', clock_timestamp())",
        )
        .bind(session_id)
        .bind(account_uuid)
        .bind(hash_token(&Uuid::new_v4().to_string()))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commercial_auth_event
             (id, account_id, action, occurred_at, metadata)
             VALUES ($1, $2, 'login_success', clock_timestamp(), '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(account_uuid)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        (tx_timestamp, clock_timestamp, session_id)
    });

    let pid_b = started_rx.await.unwrap();
    // Espera explícita a que B esté bloqueada por A (sin simular fechas).
    let mut blocked = false;
    for _ in 0..500 {
        let blockers: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)")
            .bind(pid_b)
            .fetch_one(&mut *conn_a)
            .await
            .unwrap();
        if blockers.contains(&pid_a) {
            blocked = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(blocked, "B debe quedar bloqueada por A");

    // A crea una sesión con el reloj real y confirma, liberando el lock.
    let s2_created: OffsetDateTime = sqlx::query_scalar(
        "INSERT INTO commercial_session
         (id, account_id, token_hash, created_at, expires_at, last_seen_at)
         VALUES ($1, $2, $3, clock_timestamp(),
                 clock_timestamp() + interval '1 hour', clock_timestamp())
         RETURNING created_at",
    )
    .bind(Uuid::new_v4())
    .bind(account.id.as_uuid())
    .bind(hash_token(&Uuid::new_v4().to_string()))
    .fetch_one(&mut *conn_a)
    .await
    .unwrap();
    sqlx::query("COMMIT").execute(&mut *conn_a).await.unwrap();

    let (tx_timestamp, clock_timestamp, b_session) = b_task.await.unwrap();
    // La transacción de B comenzó antes de que A creara su sesión; now() de B
    // habría quedado por detrás y violado revoked_at >= created_at.
    assert!(tx_timestamp < s2_created);
    assert!(clock_timestamp >= s2_created);

    let rows: Vec<(OffsetDateTime, Option<OffsetDateTime>)> = sqlx::query_as(
        "SELECT created_at, revoked_at FROM commercial_session
         WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(account.id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        rows.iter()
            .all(|(created, revoked)| { revoked.is_none_or(|revoked| revoked >= *created) })
    );
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_session
         WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 1, "la política de sesión única se conserva");
    let active_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM commercial_session WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(account.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_id, b_session);
    assert!(auth.authenticate(&existing.token, now).await.is_err());
}
