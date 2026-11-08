//! Persistencia del enrolamiento comercial. No conoce Axum ni DTOs HTTP.

use async_trait::async_trait;
use base64::Engine as _;
use platform_application::enrollment::{
    ApprovedEnrollment, BeginStorageOutcome, EnrollmentCompletion, EnrollmentError, EnrollmentPage,
    EnrollmentRepository, EnrollmentResult, InstallationSummary, NewEnrollmentRequest,
    ProofAttempt, SecurityEvent,
};
use platform_core::{
    enrollment::{self, DevicePublicKey, canonical_proof_message, verify_device_signature},
    ids::{CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

const CURSOR_VERSION: u8 = 1;
const INSTALLATIONS_KIND: &str = "installations";
const MAX_CURSOR_LEN: usize = 2048;
const ENROLLMENT_SCOPE: &str = "enrollment";
const BEGIN_ENDPOINT: &str = "begin";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallationCursor {
    v: u8,
    k: String,
    o: Uuid,
    n: String,
    i: Uuid,
}

pub struct PgEnrollmentRepository {
    pool: PgPool,
}

impl PgEnrollmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage<E>(_: E) -> EnrollmentError {
    EnrollmentError::Storage
}

fn insertion(error: sqlx::Error) -> EnrollmentError {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => EnrollmentError::Conflict,
        _ => EnrollmentError::Storage,
    }
}

fn encode_cursor(cursor: &InstallationCursor) -> EnrollmentResult<String> {
    let json = serde_json::to_vec(cursor).map_err(storage)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

fn decode_cursor(
    value: &str,
    organization: OrganizationId,
) -> EnrollmentResult<InstallationCursor> {
    if value.len() > MAX_CURSOR_LEN {
        return Err(EnrollmentError::Invalid);
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| EnrollmentError::Invalid)?;
    let cursor: InstallationCursor =
        serde_json::from_slice(&bytes).map_err(|_| EnrollmentError::Invalid)?;
    if cursor.v != CURSOR_VERSION
        || cursor.k != INSTALLATIONS_KIND
        || cursor.o != organization.as_uuid()
    {
        return Err(EnrollmentError::Invalid);
    }
    Ok(cursor)
}

async fn locked_authority(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
) -> EnrollmentResult<()> {
    let active_org: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM organization WHERE id = $1 AND status = 'active' FOR UPDATE",
    )
    .bind(organization.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    if active_org.is_none() {
        return Err(EnrollmentError::Forbidden);
    }

    // FOR SHARE bloquea el UPDATE de status (FOR NO KEY UPDATE) de una
    // deshabilitación concurrente; FOR KEY SHARE no lo haría.
    let active_account: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM commercial_account \
         WHERE id = $1 AND status = 'active' FOR SHARE",
    )
    .bind(actor.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    if active_account.is_none() {
        return Err(EnrollmentError::Forbidden);
    }

    let membership: Option<(String, i16)> = sqlx::query_as(
        "SELECT role, role_version FROM organization_membership \
         WHERE organization_id = $1 AND account_id = $2 AND revoked_at IS NULL",
    )
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    match membership {
        Some((role, 1)) if role == "owner" => Ok(()),
        _ => Err(EnrollmentError::Forbidden),
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: CommercialAccountId,
    organization: OrganizationId,
    action: &str,
    now: OffsetDateTime,
) -> EnrollmentResult<()> {
    sqlx::query(
        "INSERT INTO commercial_audit_event \
         (id, organization_id, actor_account_id, action, occurred_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(organization.as_uuid())
    .bind(actor.as_uuid())
    .bind(action)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(())
}

#[async_trait]
impl EnrollmentRepository for PgEnrollmentRepository {
    async fn begin_request(
        &self,
        request: &NewEnrollmentRequest,
    ) -> EnrollmentResult<BeginStorageOutcome> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if let Some(key_hash) = &request.idempotency_key_hash {
            let existing: Option<(Vec<u8>, bool)> = sqlx::query_as(
                "SELECT request_hash, secrets_delivered FROM idempotency_record \
                 WHERE scope = $1 AND endpoint = $2 AND key = $3 FOR UPDATE",
            )
            .bind(ENROLLMENT_SCOPE)
            .bind(BEGIN_ENDPOINT)
            .bind(key_hash)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?;
            if let Some((stored_hash, delivered)) = existing {
                tx.rollback().await.map_err(storage)?;
                if stored_hash != request.request_hash.to_vec() {
                    return Ok(BeginStorageOutcome::IdempotencyConflict);
                }
                if delivered {
                    return Ok(BeginStorageOutcome::RestartRequired);
                }
                return Ok(BeginStorageOutcome::IdempotencyConflict);
            }
        }

        let result = sqlx::query(
            "INSERT INTO enrollment_request \
             (id, installation_id, public_key, fingerprint, device_label, app_version, platform, \
              device_code_hash, user_code_hash, nonce_hash, status, organization_id, \
              approved_by_account_id, idempotency_scope, idempotency_key, request_hash, \
              created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', NULL, NULL, \
                     $11, $12, $13, $14, $15)",
        )
        .bind(request.id.as_uuid())
        .bind(request.installation_id.as_uuid())
        .bind(request.public_key.to_vec())
        .bind(request.fingerprint.to_vec())
        .bind(&request.device_label)
        .bind(&request.app_version)
        .bind(request.platform.as_str())
        .bind(request.device_code_hash.to_vec())
        .bind(request.user_code_hash.to_vec())
        .bind(request.nonce_hash.to_vec())
        .bind(ENROLLMENT_SCOPE)
        .bind(&request.idempotency_key_hash)
        .bind(request.request_hash.to_vec())
        .bind(request.created_at)
        .bind(request.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(insertion)?;
        debug_assert_eq!(result.rows_affected(), 1);

        if let Some(key_hash) = &request.idempotency_key_hash {
            sqlx::query(
                "INSERT INTO idempotency_record \
                 (id, scope, endpoint, key, request_hash, status, response_status, response_body, \
                  secrets_delivered, created_at, expires_at) \
                 VALUES ($1, $2, $3, $4, $5, 'completed', 201, NULL, true, $6, $7)",
            )
            .bind(Uuid::new_v4())
            .bind(ENROLLMENT_SCOPE)
            .bind(BEGIN_ENDPOINT)
            .bind(key_hash)
            .bind(request.request_hash.to_vec())
            .bind(request.created_at)
            .bind(request.expires_at)
            .execute(&mut *tx)
            .await
            .map_err(insertion)?;
        }

        tx.commit().await.map_err(storage)?;
        Ok(BeginStorageOutcome::Created(request.id))
    }

    async fn approve_request(
        &self,
        user_code: &str,
        actor: CommercialAccountId,
        organization: OrganizationId,
        now: OffsetDateTime,
    ) -> EnrollmentResult<ApprovedEnrollment> {
        let user_code_hash = enrollment::hash_secret(user_code);
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "SELECT id, installation_id, status, organization_id, approved_by_account_id, expires_at \
             FROM enrollment_request WHERE user_code_hash = $1 FOR UPDATE",
        )
        .bind(user_code_hash.to_vec())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::NotFound);
        };
        let request_id: Uuid = row.get("id");
        let installation_id: Uuid = row.get("installation_id");
        let status: String = row.get("status");
        let expires_at: OffsetDateTime = row.get("expires_at");
        if status == "approved" {
            let existing_org: Option<Uuid> = row.get("organization_id");
            let existing_actor: Option<Uuid> = row.get("approved_by_account_id");
            tx.rollback().await.map_err(storage)?;
            if existing_org == Some(organization.as_uuid())
                && existing_actor == Some(actor.as_uuid())
            {
                return Ok(ApprovedEnrollment {
                    request_id: EnrollmentRequestId::from_uuid(request_id),
                    installation_id: InstallationId::from_uuid(installation_id),
                    organization_id: organization,
                });
            }
            return Err(EnrollmentError::Conflict);
        }
        if status != "pending" {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::Expired);
        }
        if expires_at <= now {
            sqlx::query("UPDATE enrollment_request SET status = 'expired' WHERE id = $1")
                .bind(request_id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            return Err(EnrollmentError::Expired);
        }

        locked_authority(&mut tx, actor, organization).await?;
        sqlx::query(
            "UPDATE enrollment_request \
             SET status = 'approved', organization_id = $2, approved_by_account_id = $3, \
                 approved_at = $4 \
             WHERE id = $1 AND status = 'pending'",
        )
        .bind(request_id)
        .bind(organization.as_uuid())
        .bind(actor.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        audit(&mut tx, actor, organization, "enrollment_approved", now).await?;
        tx.commit().await.map_err(storage)?;

        Ok(ApprovedEnrollment {
            request_id: EnrollmentRequestId::from_uuid(request_id),
            installation_id: InstallationId::from_uuid(installation_id),
            organization_id: organization,
        })
    }

    async fn complete_proof(
        &self,
        proof: &ProofAttempt,
        now: OffsetDateTime,
    ) -> EnrollmentResult<EnrollmentCompletion> {
        let device_code_hash = enrollment::hash_secret(&proof.device_code);
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(
            "SELECT id, installation_id, public_key, fingerprint, device_label, status, \
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
            if installation_id == proof.installation_id.as_uuid()
                && stored_public_key.as_slice() == parsed_key_for_replay.bytes().as_slice()
                && replay_nonce_ok
                && replay_signature_ok
                && organization_id.is_some()
            {
                if let Some(binding) = active_binding(&mut tx, proof.installation_id).await? {
                    tx.rollback().await.map_err(storage)?;
                    return Ok(binding);
                }
            }
            tx.rollback().await.map_err(storage)?;
            if let Err(event_error) = self
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
            if let Err(event_error) = self
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
            return Err(error);
        }

        let existing_installation =
            sqlx::query("SELECT id, organization_id FROM installation WHERE id = $1 FOR UPDATE")
                .bind(installation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        if let Some(installation) = existing_installation.as_ref() {
            let existing_org: Uuid = installation.get("organization_id");
            if existing_org != organization_id {
                tx.rollback().await.map_err(storage)?;
                if let Err(event_error) = self
                    .record_security_event(&SecurityEvent {
                        kind:
                            platform_application::enrollment::SecurityEventKind::TenantChangeAttempt,
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
                 (id, organization_id, device_label, status, registered_at, last_seen_at, \
                  identity_status, enrolled_at, enrolled_by_account_id) \
                 VALUES ($1, $2, $3, 'active', $4, $4, 'active', $4, $5)",
            )
            .bind(installation_id)
            .bind(organization_id)
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
        let identity_id = if let Some(identity) = identity_row {
            let existing_key: Vec<u8> = identity.get("public_key");
            let existing_fingerprint: Vec<u8> = identity.get("fingerprint");
            if existing_key != stored_public_key || existing_fingerprint != stored_fingerprint {
                tx.rollback().await.map_err(storage)?;
                if let Err(event_error) = self
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
                    if let Err(event_error) = self
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
                 enrolled_by_account_id = $3, last_seen_at = $2 WHERE id = $1",
            )
            .bind(installation_id)
            .bind(now)
            .bind(actor.as_uuid())
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

    async fn list_installations(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        after: Option<&str>,
        limit: u32,
    ) -> EnrollmentResult<EnrollmentPage> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let member: Option<Uuid> = sqlx::query_scalar(
            "SELECT o.id FROM organization o \
             JOIN organization_membership m ON m.organization_id = o.id \
             WHERE o.id = $1 AND o.status = 'active' AND m.account_id = $2 \
               AND m.revoked_at IS NULL",
        )
        .bind(organization.as_uuid())
        .bind(actor.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        if member.is_none() {
            tx.rollback().await.map_err(storage)?;
            return Err(EnrollmentError::NotFound);
        }

        let cursor = after
            .map(|value| decode_cursor(value, organization))
            .transpose()?;
        let cursor_label = cursor.as_ref().map(|cursor| cursor.n.clone());
        let cursor_id = cursor.as_ref().map(|cursor| cursor.i);
        let rows = sqlx::query(
            "SELECT i.id, i.device_label, i.status, i.identity_status, b.bound_at, i.last_seen_at \
             FROM installation i \
             LEFT JOIN LATERAL ( \
                 SELECT bound_at FROM installation_binding \
                 WHERE installation_id = i.id AND revoked_at IS NULL \
                 ORDER BY bound_at DESC LIMIT 1 \
             ) b ON TRUE \
             WHERE i.organization_id = $1 \
               AND ($2::text IS NULL OR (i.device_label, i.id) > ($2, $3)) \
             ORDER BY i.device_label, i.id \
             LIMIT $4",
        )
        .bind(organization.as_uuid())
        .bind(cursor_label)
        .bind(cursor_id)
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;

        let mut items = Vec::with_capacity(rows.len().min(limit as usize));
        for row in rows.iter().take(limit as usize) {
            items.push(InstallationSummary {
                installation_id: InstallationId::from_uuid(row.get("id")),
                device_label: row.get("device_label"),
                status: row.get("status"),
                identity_status: row.get("identity_status"),
                bound_at: row.get("bound_at"),
                last_seen_at: row.get("last_seen_at"),
            });
        }
        let next_cursor = if rows.len() > limit as usize {
            items
                .last()
                .map(|last| {
                    encode_cursor(&InstallationCursor {
                        v: CURSOR_VERSION,
                        k: INSTALLATIONS_KIND.to_owned(),
                        o: organization.as_uuid(),
                        n: last.device_label.clone(),
                        i: last.installation_id.as_uuid(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(EnrollmentPage { items, next_cursor })
    }

    async fn record_security_event(&self, event: &SecurityEvent) -> EnrollmentResult<()> {
        sqlx::query(
            "INSERT INTO commercial_security_event \
             (id, kind, organization_id, account_id, installation_id, metadata, created_at) \
             VALUES ($1, $2, $3, $4, $5, '{}'::jsonb, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(event.kind.as_str())
        .bind(event.organization_id.map(|id| id.as_uuid()))
        .bind(event.account_id.map(|id| id.as_uuid()))
        .bind(event.installation_id.map(|id| id.as_uuid()))
        .bind(event.occurred_at)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }
}

async fn active_binding(
    tx: &mut Transaction<'_, Postgres>,
    installation: InstallationId,
) -> EnrollmentResult<Option<EnrollmentCompletion>> {
    let row = sqlx::query(
        "SELECT id, organization_id, installation_id, enrollment_request_id, \
                installation_identity_id, bound_at \
         FROM installation_binding WHERE installation_id = $1 AND revoked_at IS NULL",
    )
    .bind(installation.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    Ok(row.map(|row| EnrollmentCompletion {
        enrollment_request_id: EnrollmentRequestId::from_uuid(row.get("enrollment_request_id")),
        installation_id: InstallationId::from_uuid(row.get("installation_id")),
        organization_id: OrganizationId::from_uuid(row.get("organization_id")),
        identity_id: row.get("installation_identity_id"),
        binding_id: row.get("id"),
        bound_at: row.get("bound_at"),
    }))
}
