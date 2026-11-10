//! Escenario: concurrency.

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn revocacion_cambio_de_rol_y_archivado_concurrentes_impiden_binding() {
    use platform_application::enrollment::EnrollmentError;

    let pool = open(&test_url()).await.unwrap();

    let (organization_id, owner, proof, installation) =
        approved_proof_fixture(&pool, "Caja revocada").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET revoked_at = now() \
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization_id)
    .bind(owner.as_uuid())
    .execute(&mut *blocker)
    .await
    .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    let (organization_id, owner, proof, installation) =
        approved_proof_fixture(&pool, "Caja rol cambiado").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' \
         WHERE organization_id = $1 AND account_id = $2",
    )
    .bind(organization_id)
    .bind(owner.as_uuid())
    .execute(&mut *blocker)
    .await
    .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    let (organization_id, _, proof, installation) =
        approved_proof_fixture(&pool, "Caja organización archivada").await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM organization WHERE id = $1 FOR UPDATE")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("UPDATE organization SET status = 'archived' WHERE id = $1")
        .bind(organization_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let task = tokio::spawn(prove_in_task(pool.clone(), proof));
    wait_for_organization_lock(&pool).await;
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(EnrollmentError::EnrollmentInvalid)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn dos_proof_simultaneos_crean_un_unico_binding() {
    let pool = open(&test_url()).await.unwrap();
    let (_, _, proof, installation) = approved_proof_fixture(&pool, "Caja dos proof").await;
    let first = tokio::spawn(prove_in_task(pool.clone(), proof.clone()));
    let second = tokio::spawn(prove_in_task(pool.clone(), proof));
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert!(
        first.is_ok() || second.is_ok(),
        "al menos uno debe completar o reproducir el binding"
    );
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_binding WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bindings, 1);
    let identities: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installation_identity WHERE installation_id = $1")
            .bind(installation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identities, 1);
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM enrollment_request \
         WHERE installation_id = $1 AND status = 'completed'",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1);
}
