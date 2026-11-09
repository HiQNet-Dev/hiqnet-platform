//! Enrolamiento seguro de instalaciones.

use crate::error::{DomainError, DomainResult};
use crate::ids::{CommercialAccountId, EnrollmentRequestId, InstallationId, OrganizationId};
use base64::Engine as _;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// TTL por defecto del challenge de enrolamiento, en segundos.
pub const DEFAULT_CHALLENGE_TTL_SECONDS: u64 = 600;
/// Longitud máxima aceptada para una Idempotency-Key HTTP.
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;
/// Prefijo exacto del mensaje firmado por el dispositivo.
pub const PROOF_PREFIX: &str = "hiqnet-enrollment-proof:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentStatus {
    Pending,
    Approved,
    Completed,
    Expired,
    Cancelled,
}

impl EnrollmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Completed => "completed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "completed" => Ok(Self::Completed),
            "expired" => Ok(Self::Expired),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DomainError::EmptyName),
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Approved)
                | (Self::Pending, Self::Expired)
                | (Self::Pending, Self::Cancelled)
                | (Self::Approved, Self::Completed)
                | (Self::Approved, Self::Expired)
                | (Self::Approved, Self::Cancelled)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Expired | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnrollmentPlatform {
    Windows,
    Linux,
    Android,
    Unknown,
}

impl EnrollmentPlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Android => "android",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        match value {
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            "android" => Ok(Self::Android),
            "unknown" => Ok(Self::Unknown),
            _ => Err(DomainError::EmptyName),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePublicKey {
    encoded: String,
    bytes: [u8; 32],
}

impl DevicePublicKey {
    pub fn parse(value: &str) -> DomainResult<Self> {
        let Some(encoded_key) = value.strip_prefix("ed25519:v1:") else {
            return Err(DomainError::EmptyName);
        };
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            encoded_key,
        )
        .map_err(|_| DomainError::EmptyName)?;
        if bytes.len() != 32 {
            return Err(DomainError::EmptyName);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self {
            encoded: value.to_owned(),
            bytes: array,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    pub fn bytes(&self) -> [u8; 32] {
        self.bytes
    }

    pub fn fingerprint(&self) -> DeviceFingerprint {
        DeviceFingerprint(Sha256::digest(self.bytes).into())
    }
}

pub fn hash_secret(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

pub fn generate_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_user_code() -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    let code: String = bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) & 31] as char)
        .collect();
    format!("{}-{}", &code[..4], &code[4..])
}

pub fn validate_idempotency_key(value: &str) -> DomainResult<()> {
    if value.is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_LEN
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(DomainError::EmptyName);
    }
    Ok(())
}

pub fn canonical_proof_message(
    device_code: &str,
    installation_id: InstallationId,
    public_key: &str,
    nonce: &str,
) -> Vec<u8> {
    format!(
        "{PROOF_PREFIX}\nrequest={device_code}\ninstallation={installation_id}\nkey={public_key}\nnonce={nonce}"
    )
    .into_bytes()
}

pub fn verify_device_signature(
    public_key: &DevicePublicKey,
    message: &[u8],
    signature_b64: &str,
) -> DomainResult<()> {
    let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| DomainError::EmptyName)?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key.bytes())
        .map_err(|_| DomainError::EmptyName)?;
    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes)
        .map_err(|_| DomainError::EmptyName)?;
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|_| DomainError::EmptyName)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceFingerprint(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceCodeHash(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserCodeHash(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnrollmentNonceHash(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationIdentity {
    pub id: uuid::Uuid,
    pub installation_id: InstallationId,
    pub public_key: DevicePublicKey,
    pub fingerprint: DeviceFingerprint,
    pub generation: u32,
    pub key_version: String,
    pub created_at: OffsetDateTime,
    pub retired_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationBinding {
    pub id: uuid::Uuid,
    pub organization_id: OrganizationId,
    pub installation_id: InstallationId,
    pub enrollment_request_id: EnrollmentRequestId,
    pub installation_identity_id: uuid::Uuid,
    pub approved_by_account_id: CommercialAccountId,
    pub bound_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
    pub revoke_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_clave_ed25519_base64url() {
        let key = "ed25519:v1:".to_owned()
            + &base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, [7u8; 32]);
        let parsed = DevicePublicKey::parse(&key).unwrap();
        assert_eq!(parsed.bytes(), [7u8; 32]);
    }

    #[test]
    fn rechaza_clave_invalida() {
        assert!(DevicePublicKey::parse("ed25519:v1:AAAA").is_err());
        assert!(DevicePublicKey::parse("rsa:v1:AAAA").is_err());
    }

    #[test]
    fn serializa_estados_en_snake_case() {
        assert_eq!(EnrollmentStatus::Approved.as_str(), "approved");
    }

    #[test]
    fn canonical_proof_es_bytes_exactos() {
        let installation = InstallationId::from_uuid(uuid::Uuid::nil());
        let message = canonical_proof_message("dev", installation, "ed25519:v1:abc", "nonce");
        assert_eq!(
            String::from_utf8(message).unwrap(),
            "hiqnet-enrollment-proof:v1\nrequest=dev\ninstallation=00000000-0000-0000-0000-000000000000\nkey=ed25519:v1:abc\nnonce=nonce"
        );
    }

    #[test]
    fn firma_ed25519_valida_y_rechaza_manipulacion() {
        use ed25519_dalek::{Signer, SigningKey};
        let signing = SigningKey::generate(&mut OsRng);
        let key = DevicePublicKey::parse(&format!(
            "ed25519:v1:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing.verifying_key().as_bytes())
        ))
        .unwrap();
        let message = canonical_proof_message(
            "device",
            InstallationId::from_uuid(uuid::Uuid::nil()),
            key.as_str(),
            "nonce",
        );
        let signature = signing.sign(&message);
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes());
        verify_device_signature(&key, &message, &encoded).unwrap();
        let mut tampered = message;
        tampered[0] ^= 0x01;
        assert!(verify_device_signature(&key, &tampered, &encoded).is_err());
    }

    #[test]
    fn limita_idempotency_key_y_transiciones() {
        assert!(validate_idempotency_key("clave-segura").is_ok());
        assert!(validate_idempotency_key(&"a".repeat(MAX_IDEMPOTENCY_KEY_LEN + 1)).is_err());
        assert!(EnrollmentStatus::Pending.can_transition_to(EnrollmentStatus::Approved));
        assert!(EnrollmentStatus::Approved.can_transition_to(EnrollmentStatus::Completed));
        assert!(!EnrollmentStatus::Completed.can_transition_to(EnrollmentStatus::Approved));
        let user_code = generate_user_code();
        assert_eq!(user_code.len(), 9);
        assert_eq!(user_code.as_bytes()[4], b'-');
    }
}
