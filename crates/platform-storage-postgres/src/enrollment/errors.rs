//! Traducción de errores de persistencia del enrolamiento.

use super::*;

pub(super) fn storage<E>(_: E) -> EnrollmentError {
    EnrollmentError::Storage
}

pub(super) fn insertion(error: sqlx::Error) -> EnrollmentError {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => EnrollmentError::Conflict,
        _ => EnrollmentError::Storage,
    }
}
