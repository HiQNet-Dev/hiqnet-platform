use base64::Engine as _;
use platform_application::commercial::CommercialService;
use platform_core::{
    commercial::{CommercialError, OrganizationRole},
    ids::{CommercialAccountId, InstallationId, OrganizationId},
};
use platform_storage_postgres::{PgCommercialRepository, open};
use sqlx::{PgPool, Row};
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

async fn installation(pool: &PgPool, organization: Uuid) -> Uuid {
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

#[tokio::test]
async fn isolates_organizations_and_audits_membership_mutations() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("owner-{}@example.test", Uuid::new_v4()),
            "Dueño",
            now,
        )
        .await
        .unwrap();
    let other = service
        .create_account(
            &format!("other-{}@example.test", Uuid::new_v4()),
            "Ajeno",
            now,
        )
        .await
        .unwrap();
    let member = service
        .create_account(
            &format!("member-{}@example.test", Uuid::new_v4()),
            "Socio",
            now,
        )
        .await
        .unwrap();
    let unattributed: bool = sqlx::query_scalar(
        "SELECT actor_account_id IS NULL FROM commercial_audit_event
         WHERE subject_account_id = $1 AND action = 'account_created'",
    )
    .bind(owner.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        unattributed,
        "un alta interna no simula un actor autenticado"
    );
    let a = service
        .create_organization(owner.id, "Tienda A", now)
        .await
        .unwrap();
    let b = service
        .create_organization(other.id, "Tienda B", now)
        .await
        .unwrap();
    let a_installation = installation(&pool, a.id.as_uuid()).await;
    let b_installation = installation(&pool, b.id.as_uuid()).await;

    assert_eq!(
        service
            .get_installation(other.id, InstallationId::from_uuid(a_installation))
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .get_installation(member.id, InstallationId::from_uuid(a_installation))
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service.list_members(other.id, a.id, None, 50).await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .add_member(other.id, a.id, member.id, OrganizationRole::Member, now)
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .add_member(
                owner.id,
                a.id,
                CommercialAccountId::new(),
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service.revoke_member(owner.id, a.id, owner.id, now).await,
        Err(CommercialError::Conflict)
    );
    assert_eq!(
        service
            .change_role(owner.id, a.id, owner.id, OrganizationRole::Admin, now)
            .await,
        Err(CommercialError::Conflict)
    );

    service
        .add_member(owner.id, a.id, member.id, OrganizationRole::Member, now)
        .await
        .unwrap();
    assert_eq!(
        service
            .list_members(member.id, a.id, None, 50)
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(
        service
            .get_installation(member.id, InstallationId::from_uuid(a_installation))
            .await
            .is_ok()
    );
    assert_eq!(
        service
            .get_installation(member.id, InstallationId::from_uuid(b_installation))
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .add_member(member.id, a.id, other.id, OrganizationRole::Owner, now)
            .await,
        Err(CommercialError::Forbidden),
        "un member no puede administrar membresías"
    );
    service
        .change_role(owner.id, a.id, member.id, OrganizationRole::Admin, now)
        .await
        .unwrap();
    assert_eq!(
        service
            .change_role(owner.id, a.id, member.id, OrganizationRole::Admin, now)
            .await,
        Err(CommercialError::Conflict)
    );
    service
        .revoke_member(owner.id, a.id, member.id, now)
        .await
        .unwrap();
    assert_eq!(
        service
            .list_members(owner.id, a.id, None, 50)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        service.list_members(member.id, a.id, None, 50).await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .get_installation(member.id, InstallationId::from_uuid(a_installation))
            .await,
        Err(CommercialError::NotFound)
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM commercial_audit_event WHERE organization_id = $1",
    )
    .bind(a.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 5); // organización, dueño, alta, cambio de rol y revocación
    let transition = sqlx::query(
        "SELECT old_role, new_role FROM commercial_audit_event
         WHERE organization_id = $1 AND subject_account_id = $2
           AND action = 'membership_role_changed'",
    )
    .bind(a.id.as_uuid())
    .bind(member.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        transition.get::<Option<String>, _>("old_role").as_deref(),
        Some("member")
    );
    assert_eq!(
        transition.get::<Option<String>, _>("new_role").as_deref(),
        Some("admin")
    );
    let mut tx = pool.begin().await.unwrap();
    let tamper = sqlx::query("DELETE FROM commercial_audit_event WHERE organization_id = $1")
        .bind(a.id.as_uuid())
        .execute(&mut *tx)
        .await;
    assert!(
        tamper.is_err(),
        "los eventos de auditoría no pueden borrarse"
    );
    tx.rollback().await.unwrap();

    sqlx::query("DELETE FROM installation WHERE id IN ($1, $2)")
        .bind(a_installation)
        .bind(b_installation)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn lists_and_gets_only_active_actor_memberships() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let account = service
        .create_account(
            &format!("p023-{}@example.test", Uuid::new_v4()),
            "Actor P023",
            now,
        )
        .await
        .unwrap();
    let other = service
        .create_account(
            &format!("p023-other-{}@example.test", Uuid::new_v4()),
            "Otro P023",
            now,
        )
        .await
        .unwrap();
    let first = service
        .create_organization(account.id, "Alfa", now)
        .await
        .unwrap();
    let second = service
        .create_organization(account.id, "Beta", now)
        .await
        .unwrap();
    let foreign = service
        .create_organization(other.id, "Gamma", now)
        .await
        .unwrap();

    service
        .add_member(
            account.id,
            second.id,
            other.id,
            OrganizationRole::Member,
            now,
        )
        .await
        .unwrap();
    let own = service
        .list_organizations(account.id, None, 50)
        .await
        .unwrap();
    assert_eq!(own.items.len(), 2);
    assert_eq!(own.items[0].display_name, "Alfa");
    assert_eq!(own.items[0].role, OrganizationRole::Owner);
    assert_eq!(own.items[1].display_name, "Beta");
    assert_eq!(own.items[1].role, OrganizationRole::Owner);
    assert!(
        own.items
            .iter()
            .all(|organization| organization.status == "active")
    );

    let other_view = service
        .list_organizations(other.id, None, 50)
        .await
        .unwrap();
    assert_eq!(other_view.items.len(), 2);
    assert!(other_view.items.iter().all(|view| view.id != first.id));

    assert_eq!(
        service.get_organization(account.id, foreign.id).await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service.get_organization(other.id, first.id).await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .get_organization(account.id, first.id)
            .await
            .unwrap()
            .role,
        OrganizationRole::Owner
    );

    // Membresía revocada: deja de verse y de ser accesible.
    service
        .revoke_member(account.id, second.id, other.id, now)
        .await
        .unwrap();
    let other_after = service
        .list_organizations(other.id, None, 50)
        .await
        .unwrap();
    assert_eq!(other_after.items.len(), 1);
    assert_eq!(other_after.items[0].id, foreign.id);

    // Organización archivada: no visible ni consultable.
    sqlx::query("UPDATE organization SET status = 'archived' WHERE id = $1")
        .bind(second.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let after_archive = service
        .list_organizations(account.id, None, 50)
        .await
        .unwrap();
    assert_eq!(after_archive.items.len(), 1);
    assert_eq!(
        service.get_organization(account.id, second.id).await,
        Err(CommercialError::NotFound)
    );

    // Organización heredada sin reclamar: no visible ni reclamable.
    let legacy = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organization (id, display_name, status, created_at)
         VALUES ($1, 'Legacy', 'legacy_unclaimed', now())",
    )
    .bind(legacy)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO organization_membership
         (organization_id, account_id, role, role_version, joined_at)
         VALUES ($1, $2, 'owner', 1, now())",
    )
    .bind(legacy)
    .bind(account.id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        service
            .list_organizations(account.id, None, 50)
            .await
            .unwrap()
            .items
            .iter()
            .all(|view| view.id != OrganizationId::from_uuid(legacy))
    );
    assert_eq!(
        service
            .get_organization(account.id, OrganizationId::from_uuid(legacy))
            .await,
        Err(CommercialError::NotFound)
    );

    // Cuenta deshabilitada: sin listado ni acceso.
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .list_organizations(account.id, None, 50)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        service.get_organization(account.id, first.id).await,
        Err(CommercialError::NotFound)
    );
}

#[tokio::test]
async fn creation_rolls_back_when_audit_fails_and_rejects_disabled_account() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let account = service
        .create_account(
            &format!("p023-audit-{}@example.test", Uuid::new_v4()),
            "Auditoría P023",
            now,
        )
        .await
        .unwrap();

    let trigger_sql = format!(
        "CREATE OR REPLACE FUNCTION test_fail_org_audit_fn() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.action = 'organization_created' AND NEW.actor_account_id = '{}'::uuid THEN
             RAISE EXCEPTION 'fallo sintético de auditoría';
           END IF;
           RETURN NEW;
         END;
         $$",
        account.id
    );
    sqlx::query(&trigger_sql).execute(&pool).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER test_fail_org_audit BEFORE INSERT ON commercial_audit_event
         FOR EACH ROW EXECUTE FUNCTION test_fail_org_audit_fn()",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        service
            .create_organization(account.id, "No debe persistir", now)
            .await,
        Err(CommercialError::Storage)
    );
    sqlx::query("DROP TRIGGER test_fail_org_audit ON commercial_audit_event")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION test_fail_org_audit_fn()")
        .execute(&pool)
        .await
        .unwrap();
    let organizations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization WHERE display_name = 'No debe persistir'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(organizations, 0);
    let memberships: i64 =
        sqlx::query_scalar("SELECT count(*) FROM organization_membership WHERE account_id = $1")
            .bind(account.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(memberships, 0);

    // Deshabilitación confirmada antes de crear: no se crea ni audita.
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(account.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        service
            .create_organization(account.id, "Cuenta deshabilitada", now)
            .await,
        Err(CommercialError::NotFound)
    );
    let created: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization WHERE display_name = 'Cuenta deshabilitada'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(created, 0);
}

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

#[tokio::test]
async fn owner_administers_members_and_other_roles_are_forbidden() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-owner-{}@example.test", Uuid::new_v4()),
            "Owner B",
            now,
        )
        .await
        .unwrap();
    let admin = service
        .create_account(
            &format!("p023b-admin-{}@example.test", Uuid::new_v4()),
            "Admin B",
            now,
        )
        .await
        .unwrap();
    let member = service
        .create_account(
            &format!("p023b-member-{}@example.test", Uuid::new_v4()),
            "Member B",
            now,
        )
        .await
        .unwrap();
    let foreign = service
        .create_account(
            &format!("p023b-foreign-{}@example.test", Uuid::new_v4()),
            "Foreign B",
            now,
        )
        .await
        .unwrap();
    let organization = service
        .create_organization(owner.id, "Tienda P023B", now)
        .await
        .unwrap();
    let foreign_organization = service
        .create_organization(foreign.id, "Tienda Ajena B", now)
        .await
        .unwrap();

    service
        .add_member(
            owner.id,
            organization.id,
            admin.id,
            OrganizationRole::Admin,
            now,
        )
        .await
        .unwrap();
    service
        .add_member(
            owner.id,
            organization.id,
            member.id,
            OrganizationRole::Member,
            now,
        )
        .await
        .unwrap();

    // Cualquier miembro activo consulta; las mutaciones exigen owner.
    for account in [owner.id, admin.id, member.id] {
        assert_eq!(
            service
                .list_members(account, organization.id, None, 50)
                .await
                .unwrap()
                .items
                .len(),
            3
        );
    }
    assert_eq!(
        service
            .add_member(
                admin.id,
                organization.id,
                foreign.id,
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::Forbidden)
    );
    assert_eq!(
        service
            .change_role(
                admin.id,
                organization.id,
                member.id,
                OrganizationRole::Owner,
                now
            )
            .await,
        Err(CommercialError::Forbidden)
    );
    assert_eq!(
        service
            .revoke_member(admin.id, organization.id, member.id, now)
            .await,
        Err(CommercialError::Forbidden)
    );
    assert_eq!(
        service
            .change_role(
                member.id,
                organization.id,
                member.id,
                OrganizationRole::Owner,
                now
            )
            .await,
        Err(CommercialError::Forbidden)
    );

    // Organización ajena o inexistente: 404 neutral, nunca 403.
    assert_eq!(
        service
            .list_members(owner.id, foreign_organization.id, None, 50)
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .add_member(
                owner.id,
                foreign_organization.id,
                member.id,
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .change_role(
                owner.id,
                OrganizationId::new(),
                member.id,
                OrganizationRole::Admin,
                now
            )
            .await,
        Err(CommercialError::NotFound)
    );

    // Duplicado y reincorporación de una membresía revocada: conflicto controlado.
    assert_eq!(
        service
            .add_member(
                owner.id,
                organization.id,
                admin.id,
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::Conflict)
    );
    service
        .revoke_member(owner.id, organization.id, admin.id, now)
        .await
        .unwrap();
    assert_eq!(
        service
            .add_member(
                owner.id,
                organization.id,
                admin.id,
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::Conflict),
        "la PK organización/cuenta impide una segunda fila"
    );

    // Cuenta deshabilitada no adquiere privilegios.
    let disabled = service
        .create_account(
            &format!("p023b-disabled-{}@example.test", Uuid::new_v4()),
            "Disabled B",
            now,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE commercial_account SET status = 'disabled' WHERE id = $1")
        .bind(disabled.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        service
            .add_member(
                owner.id,
                organization.id,
                disabled.id,
                OrganizationRole::Member,
                now
            )
            .await,
        Err(CommercialError::NotFound)
    );

    // El rol efectivo se audita con la transición real.
    service
        .change_role(
            owner.id,
            organization.id,
            member.id,
            OrganizationRole::Admin,
            now,
        )
        .await
        .unwrap();
    let transition = sqlx::query(
        "SELECT old_role, new_role FROM commercial_audit_event
         WHERE organization_id = $1 AND subject_account_id = $2
           AND action = 'membership_role_changed'",
    )
    .bind(organization.id.as_uuid())
    .bind(member.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        transition.get::<Option<String>, _>("old_role").as_deref(),
        Some("member")
    );
    assert_eq!(
        transition.get::<Option<String>, _>("new_role").as_deref(),
        Some("admin")
    );
}

#[tokio::test]
async fn last_owner_and_revocation_are_protected() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-last-{}@example.test", Uuid::new_v4()),
            "Último",
            now,
        )
        .await
        .unwrap();
    let second = service
        .create_account(
            &format!("p023b-second-{}@example.test", Uuid::new_v4()),
            "Segundo",
            now,
        )
        .await
        .unwrap();
    let organization = service
        .create_organization(owner.id, "Owner único", now)
        .await
        .unwrap();

    assert_eq!(
        service
            .change_role(
                owner.id,
                organization.id,
                owner.id,
                OrganizationRole::Admin,
                now
            )
            .await,
        Err(CommercialError::Conflict)
    );
    assert_eq!(
        service
            .revoke_member(owner.id, organization.id, owner.id, now)
            .await,
        Err(CommercialError::Conflict)
    );

    // Con dos owners, uno puede ceder el rol sin dejar la organización huérfana.
    service
        .add_member(
            owner.id,
            organization.id,
            second.id,
            OrganizationRole::Owner,
            now,
        )
        .await
        .unwrap();
    service
        .change_role(
            owner.id,
            organization.id,
            owner.id,
            OrganizationRole::Admin,
            now,
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .get_organization(owner.id, organization.id)
            .await
            .unwrap()
            .role,
        OrganizationRole::Admin
    );
    assert_eq!(
        service
            .revoke_member(second.id, organization.id, second.id, now)
            .await,
        Err(CommercialError::Conflict),
        "queda un único owner y no puede revocarse"
    );

    // La revocación persistente impide acceso posterior.
    service
        .revoke_member(second.id, organization.id, owner.id, now)
        .await
        .unwrap();
    assert_eq!(
        service.get_organization(owner.id, organization.id).await,
        Err(CommercialError::NotFound)
    );
    assert_eq!(
        service
            .list_members(owner.id, organization.id, None, 50)
            .await,
        Err(CommercialError::NotFound)
    );
    let revoked_at: Option<OffsetDateTime> = sqlx::query_scalar(
        "SELECT revoked_at FROM organization_membership WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization.id.as_uuid())
    .bind(owner.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked_at.is_some());
}

#[tokio::test]
async fn membership_audit_failure_rolls_back_role_and_revocation() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-audit-{}@example.test", Uuid::new_v4()),
            "Audit B",
            now,
        )
        .await
        .unwrap();
    let member = service
        .create_account(
            &format!("p023b-audit-m-{}@example.test", Uuid::new_v4()),
            "Audit M",
            now,
        )
        .await
        .unwrap();
    let organization = service
        .create_organization(owner.id, "Auditoría B", now)
        .await
        .unwrap();
    service
        .add_member(
            owner.id,
            organization.id,
            member.id,
            OrganizationRole::Member,
            now,
        )
        .await
        .unwrap();

    let trigger_sql = format!(
        "CREATE OR REPLACE FUNCTION test_fail_member_audit_fn() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.organization_id = '{}'::uuid AND NEW.action IN
              ('membership_role_changed', 'membership_revoked') THEN
             RAISE EXCEPTION 'fallo sintético de auditoría';
           END IF;
           RETURN NEW;
         END;
         $$",
        organization.id
    );
    sqlx::query(&trigger_sql).execute(&pool).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER test_fail_member_audit BEFORE INSERT ON commercial_audit_event
         FOR EACH ROW EXECUTE FUNCTION test_fail_member_audit_fn()",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        service
            .change_role(
                owner.id,
                organization.id,
                member.id,
                OrganizationRole::Admin,
                now
            )
            .await,
        Err(CommercialError::Storage)
    );
    assert_eq!(
        service
            .revoke_member(owner.id, organization.id, member.id, now)
            .await,
        Err(CommercialError::Storage)
    );
    sqlx::query("DROP TRIGGER test_fail_member_audit ON commercial_audit_event")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION test_fail_member_audit_fn()")
        .execute(&pool)
        .await
        .unwrap();

    let role: String = sqlx::query_scalar(
        "SELECT role FROM organization_membership WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization.id.as_uuid())
    .bind(member.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role, "member");
    let revoked: Option<OffsetDateTime> = sqlx::query_scalar(
        "SELECT revoked_at FROM organization_membership WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization.id.as_uuid())
    .bind(member.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked.is_none());
}

#[tokio::test]
async fn pagination_is_stable_and_tenant_scoped() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-page-{}@example.test", Uuid::new_v4()),
            "Page B",
            now,
        )
        .await
        .unwrap();
    let foreign = service
        .create_account(
            &format!("p023b-page-f-{}@example.test", Uuid::new_v4()),
            "Page F",
            now,
        )
        .await
        .unwrap();
    for index in 0..5 {
        service
            .create_organization(
                owner.id,
                &format!("Página {index:02}"),
                now + Duration::seconds(index),
            )
            .await
            .unwrap();
    }
    service
        .create_organization(foreign.id, "Página ajena", now)
        .await
        .unwrap();

    let mut cursor: Option<String> = None;
    let mut names = Vec::new();
    loop {
        let page = service
            .list_organizations(owner.id, cursor.as_deref(), 2)
            .await
            .unwrap();
        assert!(page.items.len() <= 2);
        names.extend(page.items.into_iter().map(|view| view.display_name));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        names,
        [
            "Página 00",
            "Página 01",
            "Página 02",
            "Página 03",
            "Página 04"
        ]
    );

    let organization = service
        .create_organization(owner.id, "Miembros paginados", now)
        .await
        .unwrap();
    let mut accounts = Vec::new();
    for index in 0..5 {
        let account = service
            .create_account(
                &format!("p023b-member-page-{index}-{}@example.test", Uuid::new_v4()),
                "Miembro paginado",
                now,
            )
            .await
            .unwrap();
        service
            .add_member(
                owner.id,
                organization.id,
                account.id,
                OrganizationRole::Member,
                now + Duration::seconds(index + 10),
            )
            .await
            .unwrap();
        accounts.push(account.id);
    }

    let mut cursor: Option<String> = None;
    let mut seen = Vec::new();
    loop {
        let page = service
            .list_members(owner.id, organization.id, cursor.as_deref(), 2)
            .await
            .unwrap();
        assert!(page.items.len() <= 2);
        seen.extend(
            page.items
                .into_iter()
                .map(|membership| membership.account_id),
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 6);
    assert_eq!(seen[0], owner.id);
    assert_eq!(&seen[1..], accounts.as_slice());
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), seen.len(), "la paginación no repite ni omite");

    assert_eq!(
        service
            .list_organizations(owner.id, Some("no-es-un-cursor"), 2)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    assert_eq!(
        service
            .list_members(owner.id, organization.id, Some("no-es-un-cursor"), 2)
            .await,
        Err(CommercialError::InvalidCursor)
    );
}

#[tokio::test]
async fn concurrent_role_change_and_revocation_are_serialized() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let first = service
        .create_account(
            &format!("p023b-c1-{}@example.test", Uuid::new_v4()),
            "C1",
            now,
        )
        .await
        .unwrap();
    let second = service
        .create_account(
            &format!("p023b-c2-{}@example.test", Uuid::new_v4()),
            "C2",
            now,
        )
        .await
        .unwrap();
    let target = service
        .create_account(
            &format!("p023b-ct-{}@example.test", Uuid::new_v4()),
            "CT",
            now,
        )
        .await
        .unwrap();
    let organization = service
        .create_organization(first.id, "Concurrente", now)
        .await
        .unwrap();
    service
        .add_member(
            first.id,
            organization.id,
            second.id,
            OrganizationRole::Owner,
            now,
        )
        .await
        .unwrap();
    service
        .add_member(
            first.id,
            organization.id,
            target.id,
            OrganizationRole::Owner,
            now,
        )
        .await
        .unwrap();

    // Dos owners cambian el mismo rol a la vez: el segundo observa el cambio.
    let (left, right) = tokio::join!(
        service.change_role(
            first.id,
            organization.id,
            target.id,
            OrganizationRole::Admin,
            now
        ),
        service.change_role(
            second.id,
            organization.id,
            target.id,
            OrganizationRole::Admin,
            now
        )
    );
    let outcomes = [left, right];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| **result == Err(CommercialError::Conflict))
            .count(),
        1
    );

    // Dos revocaciones concurrentes: sólo una modifica la fila.
    let (left, right) = tokio::join!(
        service.revoke_member(first.id, organization.id, target.id, now),
        service.revoke_member(second.id, organization.id, target.id, now)
    );
    let outcomes = [left, right];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| **result == Err(CommercialError::NotFound))
            .count(),
        1
    );
    assert_eq!(
        service.get_organization(target.id, organization.id).await,
        Err(CommercialError::NotFound)
    );
}

#[tokio::test]
async fn cursors_are_bound_to_listing_and_organization() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-cursor-{}@example.test", Uuid::new_v4()),
            "Cursor B",
            now,
        )
        .await
        .unwrap();
    let member = service
        .create_account(
            &format!("p023b-cursor-m-{}@example.test", Uuid::new_v4()),
            "Cursor M",
            now,
        )
        .await
        .unwrap();
    let organization_a = service
        .create_organization(owner.id, "Cursor A", now)
        .await
        .unwrap();
    let organization_b = service
        .create_organization(owner.id, "Cursor B", now)
        .await
        .unwrap();
    service
        .create_organization(owner.id, "Cursor C", now)
        .await
        .unwrap();
    service
        .add_member(
            owner.id,
            organization_a.id,
            member.id,
            OrganizationRole::Member,
            now,
        )
        .await
        .unwrap();

    let member_page = service
        .list_members(owner.id, organization_a.id, None, 1)
        .await
        .unwrap();
    let member_cursor = member_page.next_cursor.expect("cursor de miembros");
    assert!(
        service
            .list_members(owner.id, organization_a.id, Some(&member_cursor), 1)
            .await
            .is_ok(),
        "el cursor es válido en su propio listado y organización"
    );
    assert_eq!(
        service
            .list_members(owner.id, organization_b.id, Some(&member_cursor), 1)
            .await,
        Err(CommercialError::InvalidCursor),
        "un cursor de miembros no se reutiliza en otra organización"
    );

    let organization_page = service.list_organizations(owner.id, None, 1).await.unwrap();
    let organization_cursor = organization_page
        .next_cursor
        .expect("cursor de organizaciones");
    assert_eq!(
        service
            .list_members(owner.id, organization_a.id, Some(&organization_cursor), 1)
            .await,
        Err(CommercialError::InvalidCursor),
        "un cursor de organizaciones no sirve para miembros"
    );
    assert_eq!(
        service
            .list_organizations(owner.id, Some(&member_cursor), 1)
            .await,
        Err(CommercialError::InvalidCursor),
        "un cursor de miembros no sirve para organizaciones"
    );

    let encode = |value: serde_json::Value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
    };
    let wrong_kind = encode(serde_json::json!({
        "v": 1, "k": "organizations", "n": "Cursor A", "i": Uuid::new_v4()
    }));
    assert_eq!(
        service
            .list_members(owner.id, organization_a.id, Some(&wrong_kind), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    let wrong_version = encode(serde_json::json!({
        "v": 2, "k": "members", "o": organization_a.id.as_uuid(),
        "t": 0, "i": Uuid::new_v4()
    }));
    assert_eq!(
        service
            .list_members(owner.id, organization_a.id, Some(&wrong_version), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    // Formato anterior sin versión, tipo ni organización: ya no se acepta.
    let legacy_member_cursor = encode(serde_json::json!({"t": 0, "i": Uuid::new_v4()}));
    assert_eq!(
        service
            .list_members(owner.id, organization_a.id, Some(&legacy_member_cursor), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    let legacy_organization_cursor =
        encode(serde_json::json!({"n": "Cursor A", "i": Uuid::new_v4()}));
    assert_eq!(
        service
            .list_organizations(owner.id, Some(&legacy_organization_cursor), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    let oversized = "a".repeat(2049);
    assert_eq!(
        service
            .list_organizations(owner.id, Some(&oversized), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
    assert_eq!(
        service
            .list_members(owner.id, organization_a.id, Some(&oversized), 1)
            .await,
        Err(CommercialError::InvalidCursor)
    );
}

#[tokio::test]
async fn member_listing_authorization_and_read_share_one_statement() {
    let pool = open(&isolated_url("TEST_DATABASE_URL")).await.unwrap();
    let service = CommercialService::new(PgCommercialRepository::new(pool.clone()));
    let now = OffsetDateTime::now_utc();
    let owner = service
        .create_account(
            &format!("p023b-window-{}@example.test", Uuid::new_v4()),
            "Window B",
            now,
        )
        .await
        .unwrap();
    let member = service
        .create_account(
            &format!("p023b-window-m-{}@example.test", Uuid::new_v4()),
            "Window M",
            now,
        )
        .await
        .unwrap();
    let organization = service
        .create_organization(owner.id, "Ventana", now)
        .await
        .unwrap();
    service
        .add_member(
            owner.id,
            organization.id,
            member.id,
            OrganizationRole::Member,
            now,
        )
        .await
        .unwrap();

    // Evidencia de la ventana con dos conexiones coordinadas: si autorización y
    // lectura fueran sentencias separadas, la lectura posterior a la revocación
    // confirmada seguiría devolviendo filas.
    let mut first = pool.acquire().await.unwrap();
    let authorized: Option<String> = sqlx::query_scalar(
        "SELECT mine.role
         FROM organization_membership mine
         JOIN organization o ON o.id = mine.organization_id AND o.status = 'active'
         JOIN commercial_account a ON a.id = mine.account_id AND a.status = 'active'
         WHERE mine.organization_id = $1 AND mine.account_id = $2
           AND mine.revoked_at IS NULL",
    )
    .bind(organization.id.as_uuid())
    .bind(owner.id.as_uuid())
    .fetch_optional(&mut *first)
    .await
    .unwrap();
    assert_eq!(authorized.as_deref(), Some("owner"));

    let mut second = pool.acquire().await.unwrap();
    sqlx::query(
        "UPDATE organization_membership
         SET revoked_at = GREATEST(clock_timestamp(), joined_at)
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization.id.as_uuid())
    .bind(owner.id.as_uuid())
    .execute(&mut *second)
    .await
    .unwrap();
    let stale_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_membership
         WHERE organization_id = $1 AND revoked_at IS NULL",
    )
    .bind(organization.id.as_uuid())
    .fetch_one(&mut *first)
    .await
    .unwrap();
    assert!(
        stale_rows > 0,
        "la antigua fase de lectura sin autorización veía datos tras la revocación"
    );

    // La implementación corregida reautoriza en la misma sentencia: 404 neutral.
    assert_eq!(
        service
            .list_members(owner.id, organization.id, None, 50)
            .await,
        Err(CommercialError::NotFound)
    );

    // Una revocación no confirmada no se observa: antes del commit la lectura
    // mantiene la instantánea con la membresía; después, queda denegada.
    let other_owner = service
        .create_account(
            &format!("p023b-window-o2-{}@example.test", Uuid::new_v4()),
            "Window O2",
            now,
        )
        .await
        .unwrap();
    let other_organization = service
        .create_organization(other_owner.id, "Ventana 2", now)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE organization_membership
         SET revoked_at = GREATEST(clock_timestamp(), joined_at)
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(other_organization.id.as_uuid())
    .bind(other_owner.id.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(
        service
            .list_members(other_owner.id, other_organization.id, None, 50)
            .await
            .is_ok(),
        "sin commit la revocación aún no es visible"
    );
    tx.commit().await.unwrap();
    assert_eq!(
        service
            .list_members(other_owner.id, other_organization.id, None, 50)
            .await,
        Err(CommercialError::NotFound)
    );
}
