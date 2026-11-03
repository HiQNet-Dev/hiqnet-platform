use crate::error::{DomainError, DomainResult};
use crate::ids::{InstallationId, OrganizationId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Estado observable de una instalación registrada en el control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationStatus {
    Active,
    Suspended,
    Unregistered,
}

impl InstallationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallationStatus::Active => "active",
            InstallationStatus::Suspended => "suspended",
            InstallationStatus::Unregistered => "unregistered",
        }
    }
}

/// Instalación de POS registrada. La identidad del dispositivo es responsabilidad
/// del cliente; aquí solo se conserva el hash de su clave pública o token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
    pub id: InstallationId,
    pub organization_id: OrganizationId,
    pub device_label: String,
    pub status: InstallationStatus,
    pub registered_at: OffsetDateTime,
    pub last_seen_at: Option<OffsetDateTime>,
    pub trial_started_at: Option<OffsetDateTime>,
    pub trial_ends_at: Option<OffsetDateTime>,
}

impl Installation {
    pub fn new(
        id: InstallationId,
        organization_id: OrganizationId,
        device_label: impl Into<String>,
        now: OffsetDateTime,
    ) -> DomainResult<Self> {
        let device_label = device_label.into();
        let device_label = device_label.trim();
        if device_label.is_empty() {
            return Err(DomainError::EmptyName);
        }
        Ok(Self {
            id,
            organization_id,
            device_label: device_label.to_owned(),
            status: InstallationStatus::Active,
            registered_at: now,
            last_seen_at: Some(now),
            trial_started_at: None,
            trial_ends_at: None,
        })
    }

    pub fn start_trial(&mut self, now: OffsetDateTime) -> DomainResult<()> {
        if self.trial_started_at.is_some() {
            return Err(DomainError::TrialAlreadyActivated);
        }
        self.trial_started_at = Some(now);
        self.trial_ends_at = Some(now + time::Duration::days(30));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{InstallationId, OrganizationId};

    #[test]
    fn inicia_trial_por_unica_vez() {
        let mut installation = Installation::new(
            InstallationId::new(),
            OrganizationId::new(),
            "Caja matriz",
            time::OffsetDateTime::now_utc(),
        )
        .unwrap();
        let now = time::OffsetDateTime::now_utc();
        installation.start_trial(now).unwrap();
        assert!(installation.trial_ends_at.unwrap() > now);
        assert!(matches!(
            installation.start_trial(now),
            Err(DomainError::TrialAlreadyActivated)
        ));
    }
}
