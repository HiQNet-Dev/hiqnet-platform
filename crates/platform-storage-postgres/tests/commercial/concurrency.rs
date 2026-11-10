//! Escenario: concurrency.

use super::fixtures::*;
use super::*;

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
