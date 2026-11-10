//! Escenario: control.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn control_lista_filtra_pagina_y_detalla() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let prefix = format!("c5-list-{}", Uuid::new_v4().simple());
    let base = OffsetDateTime::now_utc();
    let mut expected = Vec::new();
    for (index, label) in ["alfa", "beta", "gamma"].iter().enumerate() {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO installation \
             (id, organization_id, product_id, device_label, status, identity_status, registered_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(organization)
        .bind(product)
        .bind(format!("{prefix}-{label}"))
        .bind(if index == 1 { "suspended" } else { "active" })
        .bind(if index == 2 { "unenrolled" } else { "active" })
        .bind(base + Duration::seconds(index as i64))
        .execute(&pool)
        .await
        .unwrap();
        expected.push(id);
    }

    let service = service(&pool);
    let reader = actor(
        insert_staff(&pool, StaffRole::ReadOnly).await,
        StaffRole::ReadOnly,
    );
    let filters = InstallationFilters {
        organization_id: Some(OrganizationId::from_uuid(organization)),
        search: Some(prefix.clone()),
        ..Default::default()
    };
    let first = service
        .list(&reader, filters.clone(), Some(2), None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].id.as_uuid(), expected[0]);
    assert_eq!(first.items[1].id.as_uuid(), expected[1]);
    assert!(first.items[0].product_key.is_some());
    assert_eq!(
        first.items[1].administrative_status,
        AdministrativeStatus::Suspended
    );
    let cursor = first.next_cursor.clone().expect("cursor de continuación");
    let second = service
        .list(&reader, filters.clone(), Some(2), Some(cursor))
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].id.as_uuid(), expected[2]);
    assert_eq!(second.items[0].identity_status, IdentityStatus::Unenrolled);
    assert!(second.next_cursor.is_none());

    let suspended = service
        .list(
            &reader,
            InstallationFilters {
                status: Some(AdministrativeStatus::Suspended),
                organization_id: Some(OrganizationId::from_uuid(organization)),
                search: Some(prefix.clone()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(suspended.items.len(), 1);
    assert_eq!(suspended.items[0].id.as_uuid(), expected[1]);

    let by_product = service
        .list(
            &reader,
            InstallationFilters {
                product_id: Some(ProductId::from_uuid(product)),
                search: Some(prefix.clone()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(by_product.items.len(), 3);

    for invalid in [Some(0), Some(101)] {
        assert_eq!(
            service.list(&reader, filters.clone(), invalid, None).await,
            Err(InstallationControlError::InvalidInput)
        );
    }
    assert_eq!(
        service
            .list(
                &reader,
                filters.clone(),
                None,
                Some("no-es-un-cursor".into())
            )
            .await,
        Err(InstallationControlError::InvalidInput)
    );

    let detail = service
        .get(&reader, InstallationId::from_uuid(expected[0]))
        .await
        .unwrap();
    assert_eq!(detail.summary.id.as_uuid(), expected[0]);
    assert!(detail.current_binding.is_none());
    assert!(detail.rehost.is_none());
    assert!(
        detail.current_identity_created_at.is_none(),
        "sin identidad activa no hay fecha de generación"
    );
    assert_eq!(
        service
            .get(&reader, InstallationId::from_uuid(Uuid::new_v4()))
            .await,
        Err(InstallationControlError::NotFound)
    );
}

#[tokio::test]
async fn suspender_reactivar_son_idempotentes_y_auditan() {
    let pool = open(&test_url()).await.unwrap();
    let organization = insert_organization(&pool).await;
    let (product, _) = insert_product(&pool, "active").await;
    associate_product(&pool, organization, product, "active").await;
    let installation = insert_installation(
        &pool,
        organization,
        Some(product),
        "active",
        "active",
        OffsetDateTime::now_utc(),
    )
    .await;
    let legacy = insert_installation(
        &pool,
        organization,
        Some(product),
        "unregistered",
        "unenrolled",
        OffsetDateTime::now_utc(),
    )
    .await;

    let service = service(&pool);
    let reader = actor(
        insert_staff(&pool, StaffRole::Support).await,
        StaffRole::Support,
    );
    let manager = actor(
        insert_staff(&pool, StaffRole::Operator).await,
        StaffRole::Operator,
    );
    let by_id = InstallationId::from_uuid(installation);

    assert_eq!(
        service
            .suspend(&reader, by_id, OffsetDateTime::now_utc())
            .await,
        Err(InstallationControlError::Forbidden),
        "installations.read no concede manage"
    );
    assert_eq!(
        service
            .list(&manager, InstallationFilters::default(), None, None)
            .await
            .map(|page| !page.items.is_empty()),
        Ok(true),
        "installations.manage conserva lectura"
    );

    let suspended = service
        .suspend(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        suspended.summary.administrative_status,
        AdministrativeStatus::Suspended
    );
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended"]
    );

    // Idempotente: no duplica auditoría.
    service
        .suspend(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended"]
    );

    let reactivated = service
        .reactivate(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        reactivated.summary.administrative_status,
        AdministrativeStatus::Active
    );
    service
        .reactivate(&manager, by_id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(
        audit_actions(&pool, installation).await,
        vec!["installation_suspended", "installation_reactivated"]
    );

    assert_eq!(
        service
            .suspend(
                &manager,
                InstallationId::from_uuid(legacy),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::TransitionConflict),
        "unregistered es legacy y no transiciona"
    );
    assert_eq!(
        service
            .suspend(
                &manager,
                InstallationId::from_uuid(Uuid::new_v4()),
                OffsetDateTime::now_utc()
            )
            .await,
        Err(InstallationControlError::NotFound)
    );
}
