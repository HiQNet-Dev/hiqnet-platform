//! Casos de uso de enrolamiento comercial de instalaciones.

use async_trait::async_trait;
use platform_core::{
    enrollment::{
        self, DeviceFingerprint, DevicePublicKey, EnrollmentPlatform, EnrollmentStatus,
        MAX_IDEMPOTENCY_KEY_LEN,
    },
    ids::{CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId},
};
use time::OffsetDateTime;

#[derive(Debug, thiserror::Error)]
pub enum EnrollmentError {
    #[error(transparent)]
    Domain(#[from] platform_core::DomainError),
    #[error("solicitud de enrolamiento inválida")]
    Invalid,
    #[error("recurso de enrolamiento no encontrado")]
    NotFound,
    #[error("la autoridad comercial ya no es válida")]
    Forbidden,
    #[error("conflicto de identidad o vinculación")]
    Conflict,
    #[error("se requiere reiniciar el enrolamiento")]
    RestartRequired,
    #[error("el challenge expiró o fue consumido")]
    Expired,
    #[error("firma de prueba inválida")]
    InvalidProof,
    #[error("conflicto de idempotencia")]
    IdempotencyConflict,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type EnrollmentResult<T> = Result<T, EnrollmentError>;

#[derive(Debug, Clone)]
pub struct EnrollmentChallengeView {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub nonce: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone)]
pub struct BeginEnrollmentRequest {
    pub installation_id: InstallationId,
    pub public_key: String,
    pub device_label: String,
    pub app_version: String,
    pub platform: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApproveEnrollmentRequest {
    pub user_code: String,
    pub organization_id: OrganizationId,
    pub actor_account_id: CommercialAccountId,
}

#[derive(Debug, Clone)]
pub struct ProofAttempt {
    pub device_code: String,
    pub installation_id: InstallationId,
    pub public_key: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct EnrollmentCompletion {
    pub enrollment_request_id: EnrollmentRequestId,
    pub installation_id: InstallationId,
    pub organization_id: OrganizationId,
    pub identity_id: uuid::Uuid,
    pub binding_id: uuid::Uuid,
    pub bound_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct InstallationSummary {
    pub installation_id: InstallationId,
    pub device_label: String,
    pub status: String,
    pub identity_status: String,
    pub bound_at: Option<OffsetDateTime>,
    pub last_seen_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct EnrollmentPage {
    pub items: Vec<InstallationSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewEnrollmentRequest {
    pub id: EnrollmentRequestId,
    pub installation_id: InstallationId,
    pub public_key: [u8; 32],
    pub fingerprint: [u8; 32],
    pub device_label: String,
    pub app_version: String,
    pub platform: EnrollmentPlatform,
    pub device_code_hash: [u8; 32],
    pub user_code_hash: [u8; 32],
    pub nonce_hash: [u8; 32],
    pub request_hash: [u8; 32],
    pub idempotency_key_hash: Option<String>,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ApprovedEnrollment {
    pub request_id: EnrollmentRequestId,
    pub installation_id: InstallationId,
    pub organization_id: OrganizationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginStorageOutcome {
    Created(EnrollmentRequestId),
    RestartRequired,
    IdempotencyConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityEventKind {
    EnrollmentConflict,
    DuplicateKey,
    TenantChangeAttempt,
    ReplayRejected,
    AuthorizationRevokedDuringProof,
    IdempotencyConflict,
    CloneSuspected,
}

impl SecurityEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EnrollmentConflict => "enrollment_conflict",
            Self::DuplicateKey => "duplicate_key",
            Self::TenantChangeAttempt => "tenant_change_attempt",
            Self::ReplayRejected => "replay_rejected",
            Self::AuthorizationRevokedDuringProof => "authorization_revoked_during_proof",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::CloneSuspected => "clone_suspected",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityEvent {
    pub kind: SecurityEventKind,
    pub organization_id: Option<OrganizationId>,
    pub account_id: Option<CommercialAccountId>,
    pub installation_id: Option<InstallationId>,
    pub occurred_at: OffsetDateTime,
}

#[async_trait]
pub trait EnrollmentRepository: Send + Sync {
    /// Inserta la solicitud y su registro de idempotencia en una transacción.
    async fn begin_request(
        &self,
        request: &NewEnrollmentRequest,
    ) -> EnrollmentResult<BeginStorageOutcome>;

    /// Aprobación owner; la organización se fija en esta operación.
    async fn approve_request(
        &self,
        user_code: &str,
        actor: CommercialAccountId,
        organization: OrganizationId,
        now: OffsetDateTime,
    ) -> EnrollmentResult<ApprovedEnrollment>;

    /// Consume el challenge y crea/valida installation, identity y binding.
    async fn complete_proof(
        &self,
        proof: &ProofAttempt,
        now: OffsetDateTime,
    ) -> EnrollmentResult<EnrollmentCompletion>;

    /// Listado keyset aislado por tenant y membresía activa.
    async fn list_installations(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        after: Option<&str>,
        limit: u32,
    ) -> EnrollmentResult<EnrollmentPage>;

    /// Evento append-only de rechazo; se escribe fuera de la transacción de proof.
    async fn record_security_event(&self, event: &SecurityEvent) -> EnrollmentResult<()>;
}

pub struct EnrollmentService<R: EnrollmentRepository> {
    repository: R,
    challenge_ttl_seconds: u64,
    verification_uri: String,
}

impl<R: EnrollmentRepository> EnrollmentService<R> {
    pub fn new(repository: R, challenge_ttl_seconds: u64, verification_uri: String) -> Self {
        Self {
            repository,
            challenge_ttl_seconds,
            verification_uri,
        }
    }

    pub async fn begin_enrollment(
        &self,
        request: BeginEnrollmentRequest,
        now: OffsetDateTime,
    ) -> EnrollmentResult<EnrollmentChallengeView> {
        let public_key = DevicePublicKey::parse(&request.public_key)?;
        let platform = EnrollmentPlatform::parse(&request.platform)?;
        let device_label = validate_label(&request.device_label)?;
        let app_version = validate_app_version(&request.app_version)?;
        let idempotency_key_hash = match request.idempotency_key.as_deref() {
            Some(key) => {
                if key.len() > MAX_IDEMPOTENCY_KEY_LEN {
                    return Err(EnrollmentError::Invalid);
                }
                enrollment::validate_idempotency_key(key)?;
                Some(base64_key(&enrollment::hash_secret(key)))
            }
            None => None,
        };

        let device_code = enrollment::generate_secret();
        let user_code = enrollment::generate_user_code();
        let nonce = enrollment::generate_secret();
        let user_code_hash = enrollment::hash_secret(&user_code);
        let request_hash = request_hash(
            request.installation_id,
            &request.public_key,
            &device_label,
            &app_version,
            platform,
        );
        let draft = NewEnrollmentRequest {
            id: EnrollmentRequestId::new(),
            installation_id: request.installation_id,
            public_key: public_key.bytes(),
            fingerprint: public_key.fingerprint().0,
            device_label,
            app_version,
            platform,
            device_code_hash: enrollment::hash_secret(&device_code),
            user_code_hash,
            nonce_hash: enrollment::hash_secret(&nonce),
            request_hash,
            idempotency_key_hash,
            created_at: now,
            expires_at: now + time::Duration::seconds(self.challenge_ttl_seconds as i64),
        };

        match self.repository.begin_request(&draft).await? {
            BeginStorageOutcome::Created(_) => Ok(EnrollmentChallengeView {
                device_code,
                user_code,
                verification_uri: self.verification_uri.clone(),
                nonce,
                expires_in: self.challenge_ttl_seconds,
            }),
            BeginStorageOutcome::RestartRequired => Err(EnrollmentError::RestartRequired),
            BeginStorageOutcome::IdempotencyConflict => Err(EnrollmentError::IdempotencyConflict),
        }
    }

    pub async fn approve_enrollment(
        &self,
        request: ApproveEnrollmentRequest,
        now: OffsetDateTime,
    ) -> EnrollmentResult<ApprovedEnrollment> {
        if request.user_code.trim().is_empty() || request.user_code.len() > 32 {
            return Err(EnrollmentError::Invalid);
        }
        self.repository
            .approve_request(
                &request.user_code,
                request.actor_account_id,
                request.organization_id,
                now,
            )
            .await
    }

    pub async fn prove_enrollment(
        &self,
        proof: ProofAttempt,
        now: OffsetDateTime,
    ) -> EnrollmentResult<EnrollmentCompletion> {
        DevicePublicKey::parse(&proof.public_key)?;
        if proof.device_code.is_empty()
            || proof.device_code.len() > 256
            || proof.nonce.is_empty()
            || proof.nonce.len() > 256
            || proof.signature.is_empty()
            || proof.signature.len() > 256
        {
            return Err(EnrollmentError::Invalid);
        }
        self.repository.complete_proof(&proof, now).await
    }

    pub async fn list_installations(
        &self,
        actor: CommercialAccountId,
        organization: OrganizationId,
        after: Option<&str>,
        limit: u32,
    ) -> EnrollmentResult<EnrollmentPage> {
        if after.is_some_and(|cursor| cursor.len() > 2048) {
            return Err(EnrollmentError::Invalid);
        }
        self.repository
            .list_installations(actor, organization, after, limit.clamp(1, 100))
            .await
    }

    pub async fn record_security_event(&self, event: SecurityEvent) {
        if let Err(error) = self.repository.record_security_event(&event).await {
            tracing::warn!(
                kind = event.kind.as_str(),
                error = %error,
                "no fue posible registrar evento de seguridad de enrolamiento"
            );
        }
    }
}

fn validate_label(value: &str) -> EnrollmentResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 {
        return Err(EnrollmentError::Invalid);
    }
    Ok(value.to_owned())
}

fn validate_app_version(value: &str) -> EnrollmentResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(EnrollmentError::Invalid);
    }
    Ok(value.to_owned())
}

fn base64_key(hash: &[u8; 32]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

fn request_hash(
    installation_id: InstallationId,
    public_key: &str,
    device_label: &str,
    app_version: &str,
    platform: EnrollmentPlatform,
) -> [u8; 32] {
    let canonical = format!(
        "hiqnet-enrollment-request:v1\ninstallation={installation_id}\nkey={public_key}\nlabel={device_label}\nversion={app_version}\nplatform={}",
        platform.as_str()
    );
    enrollment::hash_secret(&canonical)
}

fn _assert_status_is_typed(_: EnrollmentStatus, _: DeviceFingerprint) {}
