//! Estado compartido de la API y traducción uniforme de errores HTTP.

use crate::*;

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<PlatformService>,
    pub commercial: Arc<CommercialService<PgCommercialRepository>>,
    pub commercial_domain: Arc<
        platform_application::commercial_domain::CommercialService<
            platform_storage_postgres::PgCommercialDomainRepository,
        >,
    >,
    pub enrollment: Arc<EnrollmentService<PgEnrollmentRepository>>,
    pub organization_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_begin_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_proof_limiter: Arc<auth::LoginLimiter>,
    pub enrollment_approve_limiter: Arc<auth::LoginLimiter>,
    pub admin_token: Option<String>,
    pub licensing_enabled: bool,
    pub db_pool: PgPool,
}

pub(crate) fn err(error: AppError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        AppError::NotFound | AppError::Domain(DomainError::InstallationNotFound) => {
            StatusCode::NOT_FOUND
        }
        AppError::Unauthorized | AppError::AdminDisabled => StatusCode::UNAUTHORIZED,
        AppError::LicensingDisabled
        | AppError::Storage
        | AppError::Domain(DomainError::LicensingDisabled) => StatusCode::SERVICE_UNAVAILABLE,
        AppError::Conflict
        | AppError::Domain(
            DomainError::DuplicateInstallation | DomainError::TrialAlreadyActivated,
        ) => StatusCode::CONFLICT,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(ErrorResponse {
            code: error.code(),
            message: error.to_string(),
        }),
    )
}

pub(crate) type HttpError = (StatusCode, Json<ErrorResponse>);

pub(crate) fn json_error(rejection: JsonRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_request",
            message: "cuerpo JSON inválido o incompatible con el contrato".into(),
        }),
    )
}

pub(crate) fn path_error(rejection: PathRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_path",
            message: "identificador de ruta inválido".into(),
        }),
    )
}

pub(crate) fn query_error(rejection: QueryRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_query",
            message: "parámetros de consulta inválidos".into(),
        }),
    )
}

pub(crate) fn path_id(
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<InstallationId, HttpError> {
    path.map(|Path(id)| InstallationId::from_uuid(id))
        .map_err(path_error)
}

pub(crate) fn admin_guard(headers: &HeaderMap, token: &Option<String>) -> Result<(), AppError> {
    let Some(expected) = token else {
        return Err(AppError::AdminDisabled);
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}
