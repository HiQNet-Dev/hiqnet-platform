//! Escenario: autoridad, auditoría y aislamiento de membranías.

use super::fixtures::*;
use super::*;

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
