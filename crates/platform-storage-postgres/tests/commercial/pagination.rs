//! Escenario: pagination.

use super::fixtures::*;
use super::*;

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
