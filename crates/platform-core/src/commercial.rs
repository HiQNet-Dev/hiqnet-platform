//! Identidad comercial, separada de usuarios locales y credenciales POS.

use crate::ids::{CommercialAccountId, OrganizationId};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommercialError {
    #[error("correo comercial inválido")]
    InvalidEmail,
    #[error("nombre comercial inválido")]
    InvalidName,
    #[error("versión o rol de organización no soportado")]
    InvalidRole,
    #[error("recurso inexistente o acceso no autorizado")]
    NotFound,
    #[error("el actor carece del rol requerido en la organización")]
    Forbidden,
    #[error("conflicto de identidad o membresía")]
    Conflict,
    #[error("cursor de paginación inválido")]
    InvalidCursor,
    #[error("no fue posible completar la operación de almacenamiento")]
    Storage,
}

pub type CommercialResult<T> = Result<T, CommercialError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationRole {
    Owner,
    Admin,
    Member,
}

impl OrganizationRole {
    pub const VERSION: i16 = 1;

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn from_storage(value: &str, version: i16) -> CommercialResult<Self> {
        match (value, version) {
            ("owner", 1) => Ok(Self::Owner),
            ("admin", 1) => Ok(Self::Admin),
            ("member", 1) => Ok(Self::Member),
            _ => Err(CommercialError::InvalidRole),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialAccount {
    pub id: CommercialAccountId,
    pub email: String,
    pub display_name: String,
    pub created_at: OffsetDateTime,
}

impl CommercialAccount {
    pub fn new(email: &str, display_name: &str, now: OffsetDateTime) -> CommercialResult<Self> {
        let email = email.trim().to_lowercase();
        let (local, domain) = email.split_once('@').ok_or(CommercialError::InvalidEmail)?;
        if local.is_empty()
            || domain.is_empty()
            || domain.contains('@')
            || email.chars().count() > 320
            || email.chars().any(char::is_whitespace)
        {
            return Err(CommercialError::InvalidEmail);
        }
        let display_name = display_name.trim();
        if display_name.is_empty() || display_name.chars().count() > 200 {
            return Err(CommercialError::InvalidName);
        }
        Ok(Self {
            id: CommercialAccountId::new(),
            email,
            display_name: display_name.to_owned(),
            created_at: now,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialOrganization {
    pub id: OrganizationId,
    pub display_name: String,
    pub created_at: OffsetDateTime,
}

impl CommercialOrganization {
    pub fn new(display_name: &str, now: OffsetDateTime) -> CommercialResult<Self> {
        let display_name = display_name.trim();
        if display_name.is_empty() || display_name.chars().count() > 200 {
            return Err(CommercialError::InvalidName);
        }
        Ok(Self {
            id: OrganizationId::new(),
            display_name: display_name.to_owned(),
            created_at: now,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationMembership {
    pub organization_id: OrganizationId,
    pub account_id: CommercialAccountId,
    pub role: OrganizationRole,
    pub joined_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identidad_normalizada_y_roles_versionados() {
        let account = CommercialAccount::new(
            "  ALGUIEN@EXAMPLE.TEST  ",
            " Negocio ",
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(account.email, "alguien@example.test");
        assert_eq!(account.display_name, "Negocio");
        assert_eq!(
            CommercialAccount::new("sin-arroba", "X", OffsetDateTime::now_utc()),
            Err(CommercialError::InvalidEmail)
        );
        assert_eq!(
            CommercialOrganization::new("  ", OffsetDateTime::now_utc()),
            Err(CommercialError::InvalidName)
        );
        assert_eq!(
            OrganizationRole::from_storage("owner", 2),
            Err(CommercialError::InvalidRole)
        );
    }
}
