//! Escenario: migration.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn upgrades_p01_data_without_granting_legacy_access() {
    let url = isolated_url("TEST_UPGRADE_DATABASE_URL");
    if let Ok(standard_url) = std::env::var("TEST_DATABASE_URL") {
        assert!(
            url != standard_url,
            "las dos bases de pruebas deben ser distintas"
        );
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    let organization = Uuid::parse_str("00000000-0000-4000-8000-000000000201").unwrap();
    let id = Uuid::parse_str("00000000-0000-4000-8000-000000000202").unwrap();
    let grant = Uuid::parse_str("00000000-0000-4000-8000-000000000203").unwrap();
    let migrations_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations')::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    let applied2: bool = if migrations_table.is_some() {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = 2 AND success)",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    } else {
        false
    };
    let fixture: bool = if applied2 {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM installation WHERE id = $1)")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
    } else {
        false
    };
    if !fixture {
        if !applied2 {
            // SQLx 0.8 no ofrece run_to: arrancar con la migración publicada intacta.
            let mut previous = sqlx::migrate!("../../migrations");
            previous.migrations = std::borrow::Cow::Owned(
                previous
                    .iter()
                    .filter(|migration| migration.version == 1)
                    .cloned()
                    .collect(),
            );
            previous.run(&pool).await.unwrap();
        } else {
            // La migración 2 ya se aplicó antes de este fixture: simular la
            // importación heredada sin conceder cuenta ni membresía.
            sqlx::query(
                "INSERT INTO organization (id, display_name, status, created_at)
                 VALUES ($1, 'Organización previa sin reclamar', 'legacy_unclaimed', now())
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(organization)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO commercial_audit_event (id, organization_id, action, occurred_at)
                 VALUES ($1, $2, 'legacy_import', now())",
            )
            .bind(Uuid::new_v4())
            .bind(organization)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO installation (id, organization_id, device_label, status, registered_at)
             VALUES ($1, $2, 'legacy-fixture', 'active', now())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(organization)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO issued_license (grant_id, installation_id, organization_id, plan,
               features_json, issued_at, valid_until, grant_json, signature_b64)
             VALUES ($1, $2, $3, 'trial', '[]'::jsonb, now(), now() + interval '1 day', '{}', 'synthetic')
             ON CONFLICT (grant_id) DO NOTHING",
        )
        .bind(grant)
        .bind(id)
        .bind(organization)
        .execute(&pool)
        .await
        .unwrap();
    }
    drop(pool);

    let upgraded = open(&url).await.unwrap();
    let rows = sqlx::query("SELECT i.id, o.status FROM installation i JOIN organization o ON i.organization_id = o.id WHERE i.id = $1")
        .bind(id)
        .fetch_one(&upgraded)
        .await
        .unwrap();
    assert_eq!(rows.get::<Uuid, _>("id"), id);
    assert_eq!(rows.get::<String, _>("status"), "legacy_unclaimed");
    let retained: i64 =
        sqlx::query_scalar("SELECT count(*) FROM issued_license WHERE grant_id = $1")
            .bind(grant)
            .fetch_one(&upgraded)
            .await
            .unwrap();
    assert_eq!(retained, 1);
    let service = CommercialService::new(PgCommercialRepository::new(upgraded.clone()));
    assert_eq!(
        service
            .get_installation(CommercialAccountId::new(), InstallationId::from_uuid(id))
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .list_members(CommercialAccountId::new(), organization.into(), None, 50)
            .await,
        Err(CommercialError::NotFound)
    );
    let legacy_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM commercial_audit_event WHERE organization_id = $1 AND action = 'legacy_import'")
        .bind(organization).fetch_one(&upgraded).await.unwrap();
    assert_eq!(legacy_audit, 1);
    open(&url)
        .await
        .expect("las migraciones son idempotentes al reabrir");
}

#[tokio::test]
async fn database_enforces_tenant_links_roles_and_indexes() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let constraints: Vec<String> = sqlx::query_scalar(
        "SELECT conname FROM pg_constraint
         WHERE conname IN ('installation_organization_id_fkey',
                           'issued_license_installation_org_fkey',
                           'organization_membership_pkey',
                           'commercial_account_email_canonical')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(constraints.len(), 4);
    let indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_indexes WHERE schemaname = 'public'
         AND indexname IN ('organization_membership_account_active_idx',
                           'commercial_audit_event_org_time_idx',
                           'commercial_audit_event_subject_time_idx')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(indexes, 3);

    let mut tx = pool.begin().await.unwrap();
    let without_org = sqlx::query(
        "INSERT INTO installation (id, organization_id, device_label, status, registered_at)
         VALUES ($1, $2, 'orphan-test', 'active', now())",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(&mut *tx)
    .await;
    assert!(without_org.is_err());
    tx.rollback().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let org_a = Uuid::new_v4();
    let org_b = Uuid::new_v4();
    let installation_id = Uuid::new_v4();
    for org in [org_a, org_b] {
        sqlx::query("INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, 'prueba', 'active', now())")
            .bind(org).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO installation (id, organization_id, device_label, status, registered_at) VALUES ($1, $2, 'prueba', 'active', now())")
        .bind(installation_id).bind(org_a).execute(&mut *tx).await.unwrap();
    let wrong_tenant = sqlx::query(
        "INSERT INTO issued_license (grant_id, installation_id, organization_id, plan,
           features_json, issued_at, valid_until, grant_json, signature_b64)
         VALUES ($1, $2, $3, 'trial', '[]'::jsonb, now(), now() + interval '1 day', '{}', 'synthetic')",
    )
    .bind(Uuid::new_v4())
    .bind(installation_id)
    .bind(org_b)
    .execute(&mut *tx)
    .await;
    assert!(wrong_tenant.is_err());
    tx.rollback().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let account_id = Uuid::new_v4();
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO commercial_account (id, email, display_name, created_at) VALUES ($1, $2, 'prueba', now())")
        .bind(account_id).bind(format!("{account_id}@example.test"))
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO organization (id, display_name, status, created_at) VALUES ($1, 'prueba', 'active', now())")
        .bind(org_id).execute(&mut *tx).await.unwrap();
    let invalid_role = sqlx::query("INSERT INTO organization_membership (organization_id, account_id, role, role_version, joined_at) VALUES ($1, $2, 'superuser', 2, now())")
        .bind(org_id).bind(account_id).execute(&mut *tx).await;
    assert!(invalid_role.is_err());
    tx.rollback().await.unwrap();
}
