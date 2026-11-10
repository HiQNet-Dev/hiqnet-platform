//! Verificación criptográfica del proof y vinculación de la instalación.
//!
//! La transacción, el bloqueo `FOR UPDATE` y el orden de verificación son los
//! mismos que tenía el método original; sólo cambia de archivo.

use super::*;

pub(super) async fn complete_proof(
    repo: &PgEnrollmentRepository,
    proof: &ProofAttempt,
    now: OffsetDateTime,
) -> EnrollmentResult<EnrollmentCompletion> {
    let device_code_hash = enrollment::hash_secret(&proof.device_code);
    let mut tx = repo.pool.begin().await.map_err(storage)?;
    let row = sqlx::query(
        "SELECT id, installation_id, product_id, public_key, fingerprint, device_label, status, \
                    organization_id, approved_by_account_id, nonce_hash, expires_at, consumed_at \
             FROM enrollment_request WHERE device_code_hash = $1 FOR UPDATE",
    )
    .bind(device_code_hash.to_vec())
    .fetch_optional(&mut *tx)
    .await
    .map_err(storage)?;
    let Some(row) = row else {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::NotFound);
    };
    let request_id: Uuid = row.get("id");
    let installation_id: Uuid = row.get("installation_id");
    let request_product_id: Option<Uuid> = row.get("product_id");
    let stored_public_key: Vec<u8> = row.get("public_key");
    let stored_fingerprint: Vec<u8> = row.get("fingerprint");
    let device_label: String = row.get("device_label");
    let status: String = row.get("status");
    let organization_id: Option<Uuid> = row.get("organization_id");
    let approved_by: Option<Uuid> = row.get("approved_by_account_id");
    let nonce_hash: Vec<u8> = row.get("nonce_hash");
    let expires_at: OffsetDateTime = row.get("expires_at");
    let consumed_at: Option<OffsetDateTime> = row.get("consumed_at");

    let parsed_key_for_replay = DevicePublicKey::parse(&proof.public_key)?;
    if status == "completed" {
        let replay_message = canonical_proof_message(
            &proof.device_code,
            proof.installation_id,
            &proof.public_key,
            &proof.nonce,
        );
        let replay_nonce_ok = nonce_hash.as_slice() == enrollment::hash_secret(&proof.nonce);
        let replay_signature_ok =
            verify_device_signature(&parsed_key_for_replay, &replay_message, &proof.signature)
                .is_ok();
        // Una identidad retirada (rehost) no puede reclamar el binding vigente:
        // sólo la clave criptográfica activa conserva la idempotencia del proof.
        let active_public_key: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT public_key FROM installation_identity \
                 WHERE installation_id = $1 AND retired_at IS NULL",
        )
        .bind(installation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let replay_uses_active_key =
            active_public_key.as_deref() == Some(stored_public_key.as_slice());
        if installation_id == proof.installation_id.as_uuid()
            && stored_public_key.as_slice() == parsed_key_for_replay.bytes().as_slice()
            && replay_nonce_ok
            && replay_signature_ok
            && replay_uses_active_key
            && organization_id.is_some()
        {
            if let Some(binding) = active_binding(&mut tx, proof.installation_id).await? {
                tx.rollback().await.map_err(storage)?;
                return Ok(binding);
            }
        }
        tx.rollback().await.map_err(storage)?;
        if let Err(event_error) = repo
            .record_security_event(&SecurityEvent {
                kind: platform_application::enrollment::SecurityEventKind::ReplayRejected,
                organization_id: organization_id.map(OrganizationId::from_uuid),
                account_id: approved_by.map(CommercialAccountId::from_uuid),
                installation_id: None,
                occurred_at: now,
            })
            .await
        {
            tracing::warn!(error = %event_error, "fallo al registrar evento de seguridad de replay");
        }
        return Err(EnrollmentError::Conflict);
    }
    if status == "expired" || status == "cancelled" {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Expired);
    }
    if status != "approved" || consumed_at.is_some() {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Conflict);
    }
    if expires_at <= now {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Expired);
    }
    let parsed_key = parsed_key_for_replay;
    if parsed_key.bytes().as_slice() != stored_public_key.as_slice()
        || nonce_hash.as_slice() != enrollment::hash_secret(&proof.nonce)
        || installation_id != proof.installation_id.as_uuid()
    {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::InvalidProof);
    }
    let message = canonical_proof_message(
        &proof.device_code,
        proof.installation_id,
        &proof.public_key,
        &proof.nonce,
    );
    verify_device_signature(&parsed_key, &message, &proof.signature)?;

    let Some(organization_id) = organization_id else {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Forbidden);
    };
    let Some(approved_by) = approved_by else {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Forbidden);
    };
    let organization = OrganizationId::from_uuid(organization_id);
    let actor = CommercialAccountId::from_uuid(approved_by);
    if let Err(error) = locked_authority(&mut tx, actor, organization).await {
        tx.rollback().await.map_err(storage)?;
        if let Err(event_error) = repo
                .record_security_event(&SecurityEvent {
                    kind:
                        platform_application::enrollment::SecurityEventKind::AuthorizationRevokedDuringProof,
                    organization_id: Some(organization),
                    account_id: Some(actor),
                    installation_id: None,
                    occurred_at: now,
                })
                .await
            {
                tracing::warn!(error = %event_error, "fallo al registrar revocación de autoridad");
            }
        // En proof, una aprobación que decayó no es un 403 de rol actual:
        // se normaliza como 409 enrollment_invalid sin revelar la causa.
        return Err(match error {
            EnrollmentError::Forbidden => EnrollmentError::EnrollmentInvalid,
            other => other,
        });
    }

    let existing_installation = sqlx::query(
        "SELECT id, organization_id, product_id FROM installation WHERE id = $1 FOR UPDATE",
    )
    .bind(installation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(storage)?;
    if let Some(installation) = existing_installation.as_ref() {
        let existing_org: Uuid = installation.get("organization_id");
        if existing_org != organization_id {
            tx.rollback().await.map_err(storage)?;
            if let Err(event_error) = repo
                .record_security_event(&SecurityEvent {
                    kind: platform_application::enrollment::SecurityEventKind::TenantChangeAttempt,
                    organization_id: Some(organization),
                    account_id: Some(actor),
                    installation_id: Some(InstallationId::from_uuid(installation_id)),
                    occurred_at: now,
                })
                .await
            {
                tracing::warn!(error = %event_error, "fallo al registrar cambio de tenant");
            }
            return Err(EnrollmentError::Conflict);
        }
    }

    let existing_product = existing_installation
        .as_ref()
        .and_then(|row| row.get::<Option<Uuid>, _>("product_id"));
    let effective_product = match (request_product_id, existing_product) {
        (Some(requested), Some(existing)) if requested != existing => {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::Conflict);
        }
        (Some(requested), _) => requested,
        (None, Some(existing)) => existing,
        (None, None) => {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::EnrollmentInvalid);
        }
    };
    let product = ProductId::from_uuid(effective_product);
    let status = product_status(&mut tx, product).await?;
    let product_allowed = if existing_installation.is_some() {
        matches!(
            status,
            LifecycleStatus::Active | LifecycleStatus::Maintenance
        )
    } else {
        matches!(status, LifecycleStatus::Active)
    };
    if !product_allowed {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::EnrollmentInvalid);
    }
    if !organization_product_active(&mut tx, organization, product).await? {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::EnrollmentInvalid);
    }

    let rehost_row = sqlx::query(
        "SELECT id, status, expires_at, organization_id, product_id, \
                    authorized_by_staff_account_id \
             FROM installation_rehost_authorization \
             WHERE enrollment_request_id = $1 FOR UPDATE",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(storage)?;
    let mut rehost_staff = None;
    let rehost_id = if let Some(row) = rehost_row {
        let rehost_status: String = row.get("status");
        let rehost_expires: OffsetDateTime = row.get("expires_at");
        let rehost_org: Uuid = row.get("organization_id");
        let rehost_product: Option<Uuid> = row.get("product_id");
        if rehost_status == "consumed" {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::Conflict);
        }
        if rehost_status == "expired" || rehost_status == "cancelled" {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::EnrollmentInvalid);
        }
        if rehost_expires <= now {
            sqlx::query(
                "UPDATE installation_rehost_authorization SET status = 'expired' \
                     WHERE id = $1 AND status = 'pending'",
            )
            .bind(row.get::<Uuid, _>("id"))
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            return Err(EnrollmentError::EnrollmentInvalid);
        }
        if rehost_org != organization_id
            || rehost_product.is_some_and(|value| value != effective_product)
        {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::Conflict);
        }
        rehost_staff = Some(row.get::<Uuid, _>("authorized_by_staff_account_id"));
        Some(row.get::<Uuid, _>("id"))
    } else {
        None
    };

    let existing_binding =
            sqlx::query("SELECT id FROM installation_binding WHERE installation_id = $1 AND revoked_at IS NULL FOR UPDATE")
                .bind(installation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
    if existing_binding.is_some() {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Conflict);
    }

    if existing_installation.is_none() {
        sqlx::query(
                "INSERT INTO installation \
                 (id, organization_id, product_id, device_label, status, registered_at, last_seen_at, \
                  identity_status, enrolled_at, enrolled_by_account_id) \
                 VALUES ($1, $2, $3, $4, 'active', $5, $5, 'active', $5, $6)",
            )
            .bind(installation_id)
            .bind(organization_id)
            .bind(effective_product)
            .bind(&device_label)
            .bind(now)
            .bind(actor.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(insertion)?;
    }

    let identity_row = sqlx::query(
            "SELECT id, public_key, fingerprint, generation \
             FROM installation_identity WHERE installation_id = $1 AND retired_at IS NULL FOR UPDATE",
        )
        .bind(installation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
    if rehost_id.is_some() && identity_row.is_some() {
        tx.rollback().await.map_err(storage)?;
        return Err(EnrollmentError::Conflict);
    }
    let identity_id = if let Some(identity) = identity_row {
        let existing_key: Vec<u8> = identity.get("public_key");
        let existing_fingerprint: Vec<u8> = identity.get("fingerprint");
        if existing_key != stored_public_key || existing_fingerprint != stored_fingerprint {
            tx.rollback().await.map_err(storage)?;
            if let Err(event_error) = repo
                .record_security_event(&SecurityEvent {
                    kind: platform_application::enrollment::SecurityEventKind::DuplicateKey,
                    organization_id: Some(organization),
                    account_id: Some(actor),
                    installation_id: Some(InstallationId::from_uuid(installation_id)),
                    occurred_at: now,
                })
                .await
            {
                tracing::warn!(error = %event_error, "fallo al registrar clave duplicada");
            }
            return Err(EnrollmentError::Conflict);
        }
        let _ = existing_fingerprint;
        identity.get::<Uuid, _>("id")
    } else {
        let generation: i32 = sqlx::query_scalar(
                "SELECT COALESCE(max(generation), 0) + 1 FROM installation_identity WHERE installation_id = $1",
            )
            .bind(installation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        let identity_id = Uuid::new_v4();
        let inserted = sqlx::query(
                "INSERT INTO installation_identity \
                 (id, installation_id, public_key, fingerprint, generation, key_version, created_at) \
                 VALUES ($1, $2, $3, $4, $5, 'ed25519:v1', $6)",
            )
            .bind(identity_id)
            .bind(installation_id)
            .bind(stored_public_key.clone())
            .bind(stored_fingerprint.clone())
            .bind(generation)
            .bind(now)
            .execute(&mut *tx)
            .await;
        if let Err(error) = inserted {
            if matches!(&error, sqlx::Error::Database(db) if db.is_unique_violation()) {
                tx.rollback().await.map_err(storage)?;
                if let Err(event_error) = repo
                    .record_security_event(&SecurityEvent {
                        kind: platform_application::enrollment::SecurityEventKind::DuplicateKey,
                        organization_id: Some(organization),
                        account_id: Some(actor),
                        // La instalación nueva se revierte con la transacción;
                        // no puede usarse como FK del evento.
                        installation_id: None,
                        occurred_at: now,
                    })
                    .await
                {
                    tracing::warn!(error = %event_error, "fallo al registrar clave duplicada");
                }
                return Err(EnrollmentError::Conflict);
            }
            return Err(storage(error));
        }
        identity_id
    };

    let binding_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installation_binding \
             (id, organization_id, installation_id, enrollment_request_id, \
              installation_identity_id, approved_by_account_id, bound_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(binding_id)
    .bind(organization_id)
    .bind(installation_id)
    .bind(request_id)
    .bind(identity_id)
    .bind(approved_by)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(insertion)?;

    if let Some(authorization_id) = rehost_id {
        let consumed = sqlx::query(
            "UPDATE installation_rehost_authorization \
                 SET status = 'consumed', consumed_at = $2 \
                 WHERE id = $1 AND status = 'pending'",
        )
        .bind(authorization_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        if consumed.rows_affected() != 1 {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::Conflict);
        }
        // El cierre del rehost se audita en la misma transacción que la nueva
        // generación; si falla, no queda completion parcial.
        let staff_actor = rehost_staff.ok_or(EnrollmentError::Storage)?;
        sqlx::query(
            "INSERT INTO staff_audit_event \
                 (id, staff_account_id, action, resource_type, resource_id, organization_id, \
                  product_id, result, occurred_at, metadata) \
                 VALUES ($1, $2, 'installation_rehost_completed', 'installation', $3, $4, $5, \
                         'success', $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(staff_actor)
        .bind(installation_id)
        .bind(organization_id)
        .bind(effective_product)
        .bind(now)
        .bind(serde_json::json!({
            "enrollment_request_id": request_id.to_string(),
            "identity_id": identity_id.to_string(),
        }))
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        sqlx::query(
            "INSERT INTO commercial_security_event \
                 (id, kind, organization_id, account_id, installation_id, metadata, created_at) \
                 VALUES ($1, 'rehost_performed', $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(organization_id)
        .bind(approved_by)
        .bind(installation_id)
        .bind(serde_json::json!({
            "enrollment_request_id": request_id.to_string(),
        }))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    }

    sqlx::query(
        "UPDATE enrollment_request SET status = 'completed', consumed_at = $2 \
             WHERE id = $1 AND status = 'approved' AND consumed_at IS NULL",
    )
    .bind(request_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(storage)?;
    if existing_installation.is_some() {
        sqlx::query(
            "UPDATE installation SET identity_status = 'active', enrolled_at = $2, \
                 enrolled_by_account_id = $3, last_seen_at = $2, \
                 status = CASE WHEN status = 'unregistered' THEN 'active' ELSE status END, \
                 product_id = COALESCE(product_id, $4) WHERE id = $1",
        )
        .bind(installation_id)
        .bind(now)
        .bind(actor.as_uuid())
        .bind(effective_product)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    }
    audit(
        &mut tx,
        actor,
        organization,
        "installation_identity_registered",
        now,
    )
    .await?;
    audit(&mut tx, actor, organization, "installation_enrolled", now).await?;
    tx.commit().await.map_err(storage)?;

    Ok(EnrollmentCompletion {
        enrollment_request_id: EnrollmentRequestId::from_uuid(request_id),
        installation_id: InstallationId::from_uuid(installation_id),
        organization_id: organization,
        identity_id,
        binding_id,
        bound_at: now,
    })
}
