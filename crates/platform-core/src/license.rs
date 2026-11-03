use crate::error::{DomainError, DomainResult};
use crate::ids::{GrantId, InstallationId, OrganizationId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Versión del documento de autorización. Debe coincidir con el verificador del POS.
pub const LICENSE_DOC_VERSION: u32 = 1;

/// Autorización comercial firmada por HiQNet Platform. El POS la verifica con la
/// clave pública embebida.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LicenseGrant {
    pub version: u32,
    pub grant_id: GrantId,
    pub installation_id: InstallationId,
    pub organization_id: OrganizationId,
    pub plan: String,
    pub features: Vec<String>,
    pub issued_at: OffsetDateTime,
    pub valid_until: OffsetDateTime,
}

impl LicenseGrant {
    pub fn to_canonical_json(&self) -> DomainResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|_| DomainError::LicensingDisabled)
    }

    pub fn from_canonical_json(bytes: &[u8]) -> DomainResult<Self> {
        let grant: Self =
            serde_json::from_slice(bytes).map_err(|_| DomainError::LicensingDisabled)?;
        if grant.version != LICENSE_DOC_VERSION {
            return Err(DomainError::CommercialStateBlocked);
        }
        Ok(grant)
    }
}

/// Registro persistido de una autorización emitida (auditoría de plataforma).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedLicense {
    pub grant_id: GrantId,
    pub installation_id: InstallationId,
    pub organization_id: OrganizationId,
    pub plan: String,
    pub features_json: String,
    pub issued_at: OffsetDateTime,
    pub valid_until: OffsetDateTime,
    pub grant_json: String,
    pub signature_b64: String,
}

impl IssuedLicense {
    pub fn validate_validity(
        issued_at: OffsetDateTime,
        valid_until: OffsetDateTime,
    ) -> DomainResult<()> {
        if valid_until <= issued_at {
            return Err(DomainError::InvalidValidity);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{GrantId, InstallationId, OrganizationId};

    #[test]
    fn rechaza_vigencia_invertida() {
        let now = time::OffsetDateTime::now_utc();
        assert!(IssuedLicense::validate_validity(now, now - time::Duration::days(1)).is_err());
        assert!(IssuedLicense::validate_validity(now, now + time::Duration::days(1)).is_ok());
    }

    #[test]
    fn serializa_y_deserializa_grant() {
        let grant = LicenseGrant {
            version: LICENSE_DOC_VERSION,
            grant_id: GrantId::new(),
            installation_id: InstallationId::new(),
            organization_id: OrganizationId::new(),
            plan: "trial".into(),
            features: vec!["catalog".into()],
            issued_at: time::OffsetDateTime::now_utc(),
            valid_until: time::OffsetDateTime::now_utc() + time::Duration::days(30),
        };
        let json = grant.to_canonical_json().unwrap();
        let parsed = LicenseGrant::from_canonical_json(&json).unwrap();
        assert_eq!(parsed, grant);
    }
}
