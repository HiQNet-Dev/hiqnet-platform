//! Autenticación comercial con Argon2id y sesiones persistentes.
//!
//! Este módulo no conoce HTTP ni SQLx. El token de sesión nunca se guarda en
//! claro: sólo su hash SHA-256 llega a persistencia.

use crate::ids::CommercialAccountId;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("credenciales inválidas")]
    InvalidCredentials,
    #[error("la nueva contraseña no cumple la política")]
    WeakPassword,
    #[error("la credencial cambió durante la operación")]
    Conflict,
    #[error("demasiados intentos; inténtalo más tarde")]
    RateLimited,
    #[error("sesión inexistente, expirada o revocada")]
    InvalidSession,
    #[error("no fue posible completar la operación de almacenamiento")]
    Storage,
}

/// Política mínima de contraseñas: 12–128 caracteres, al menos una letra y un
/// dígito, sin caracteres de control. No exige clases de símbolos adicionales.
pub const MIN_PASSWORD_CHARS: usize = 12;
pub const MAX_PASSWORD_CHARS: usize = 128;

pub fn validate_password(password: &str) -> AuthResult<()> {
    let length = password.chars().count();
    if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&length)
        || password.chars().any(char::is_control)
        || !password.chars().any(char::is_alphabetic)
        || !password.chars().any(|value| value.is_ascii_digit())
    {
        return Err(AuthError::WeakPassword);
    }
    Ok(())
}

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialCredential {
    pub account_id: CommercialAccountId,
    pub password_hash: String,
    pub password_changed_at: OffsetDateTime,
}

impl CommercialCredential {
    pub fn new(
        account_id: CommercialAccountId,
        password_hash: String,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            account_id,
            password_hash,
            password_changed_at: now,
        }
    }
}

/// Cuenta comercial activa resuelta por correo normalizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialIdentity {
    pub account_id: CommercialAccountId,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSession {
    pub session_id: uuid::Uuid,
    pub account_id: CommercialAccountId,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// Sesión recién emitida. `token` sólo se devuelve al cliente una vez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    pub token: String,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn politica_de_contrasenas_rechaza_debiles() {
        assert_eq!(validate_password("corta1"), Err(AuthError::WeakPassword));
        assert_eq!(
            validate_password("sololetraslargas"),
            Err(AuthError::WeakPassword)
        );
        assert_eq!(
            validate_password("1234567890123"),
            Err(AuthError::WeakPassword)
        );
        assert_eq!(
            validate_password("con\ttabulador123"),
            Err(AuthError::WeakPassword)
        );
        assert_eq!(validate_password("frase segura 123"), Ok(()));
    }
}
