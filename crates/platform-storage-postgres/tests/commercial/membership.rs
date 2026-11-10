//! Escenario: membranías básicas.

use super::fixtures::*;
use super::*;

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

    // Desde 0009 el historial de instalaciones es inmutable: la limpieza no puede borrarlo.
    assert!(
        sqlx::query("DELETE FROM installation WHERE id IN ($1, $2)")
            .bind(a_installation)
            .bind(b_installation)
            .execute(&pool)
            .await
            .is_err(),
        "la instalación no admite eliminación física"
    );
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
