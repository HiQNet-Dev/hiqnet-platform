//! Implementación del puerto `EnrollmentRepository`.
//!
//! Rust no permite repartir una implementación de trait entre módulos; el bloque
//! conserva el orden de bloqueos, verificaciones y commits de cada transacción.

use super::*;

#[async_trait]
impl EnrollmentRepository for PgEnrollmentRepository {
    async fn product_for_enrollment(
        &self,
        product_key: &str,
    ) -> EnrollmentResult<Option<EnrollmentProduct>> {
        let row = sqlx::query("SELECT id, status FROM product WHERE key = $1")
            .bind(product_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let status: String = row.get("status");
        Ok(Some(EnrollmentProduct {
            id: ProductId::from_uuid(row.get("id")),
            status: status.parse().map_err(|_| EnrollmentError::Invalid)?,
        }))
    }

    async fn installation_attribution(
        &self,
        id: InstallationId,
    ) -> EnrollmentResult<InstallationAttribution> {
        let product: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT product_id FROM installation WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(match product {
            None => InstallationAttribution::Missing,
            Some(None) => InstallationAttribution::Legacy,
            Some(Some(product)) => InstallationAttribution::Product(ProductId::from_uuid(product)),
        })
    }

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
              created_at, expires_at, product_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', NULL, NULL, \
                     $11, $12, $13, $14, $15, $16)",
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
        .bind(request.product_id.as_uuid())
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
            "SELECT id, installation_id, product_id, status, organization_id, approved_by_account_id, expires_at \
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
        if let Some(product_id) = row.get::<Option<Uuid>, _>("product_id") {
            let product = ProductId::from_uuid(product_id);
            if !organization_product_active(&mut tx, organization, product).await? {
                tx.rollback().await.map_err(storage)?;
                return Err(EnrollmentError::EnrollmentInvalid);
            }
            let status = product_status(&mut tx, product).await?;
            let attribution: Option<Option<Uuid>> =
                sqlx::query_scalar("SELECT product_id FROM installation WHERE id = $1")
                    .bind(installation_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage)?;
            let allowed = match attribution {
                None => matches!(status, LifecycleStatus::Active),
                Some(None) => matches!(
                    status,
                    LifecycleStatus::Active | LifecycleStatus::Maintenance
                ),
                // Una instalación ya atribuida sólo admite su mismo Product.
                Some(Some(installed)) => {
                    installed == product_id
                        && matches!(
                            status,
                            LifecycleStatus::Active | LifecycleStatus::Maintenance
                        )
                }
            };
            if !allowed {
                tx.rollback().await.map_err(storage)?;
                return Err(EnrollmentError::EnrollmentInvalid);
            }
        }
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
        super::proof::complete_proof(self, proof, now).await
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
