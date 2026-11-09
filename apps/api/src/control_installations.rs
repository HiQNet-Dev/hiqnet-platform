//! Control administrativo de instalaciones POS (staffSession, sólo Control).

use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use platform_application::installation_control::{
    AdministrativeStatus, ControlActor, IdentityStatus, InstallationControlError,
    InstallationDetail, InstallationFilters, InstallationSummary, RehostAuthorizationView,
};
use platform_core::ids::{EnrollmentRequestId, InstallationId, OrganizationId, ProductId};

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn installation_error(error: InstallationControlError) -> HttpError {
    let (status, code, message) = match error {
        InstallationControlError::NotFound => {
            (StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        InstallationControlError::Forbidden => (
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        InstallationControlError::InvalidInput => (
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        InstallationControlError::TransitionConflict => (
            StatusCode::CONFLICT,
            "transition_conflict",
            "la transición de estado no está permitida",
        ),
        InstallationControlError::RehostConflict => (
            StatusCode::CONFLICT,
            "rehost_conflict",
            "conflicto de autorización de rehost",
        ),
        InstallationControlError::RehostExpired => (
            StatusCode::CONFLICT,
            "rehost_expired",
            "la autorización de rehost expiró",
        ),
        InstallationControlError::ProductUnavailable => (
            StatusCode::CONFLICT,
            "product_unavailable",
            "el producto no está disponible para la instalación",
        ),
        InstallationControlError::Storage => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    };
    auth::error(status, code, message)
}

fn query_error(_rejection: QueryRejection) -> HttpError {
    auth::error(
        StatusCode::BAD_REQUEST,
        "validation_error",
        "solicitud inválida",
    )
}

fn path_error(_rejection: PathRejection) -> HttpError {
    auth::error(
        StatusCode::BAD_REQUEST,
        "invalid_path",
        "identificador de ruta inválido",
    )
}

async fn actor(auth_state: &AuthState, context: &StaffContext) -> Result<ControlActor, HttpError> {
    let identity = auth_state
        .staff
        .service
        .identity(context.session.account_id)
        .await
        .map_err(crate::staff::staff_error)?;
    Ok(ControlActor::new(identity.account_id, identity.role))
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdministrativeStatusDto {
    Active,
    Suspended,
    Unregistered,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatusDto {
    Unenrolled,
    Active,
    Retired,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RehostStatusDto {
    Pending,
    Consumed,
    Expired,
    Cancelled,
}

impl From<AdministrativeStatus> for AdministrativeStatusDto {
    fn from(value: AdministrativeStatus) -> Self {
        match value {
            AdministrativeStatus::Active => Self::Active,
            AdministrativeStatus::Suspended => Self::Suspended,
            AdministrativeStatus::Unregistered => Self::Unregistered,
        }
    }
}

impl From<IdentityStatus> for IdentityStatusDto {
    fn from(value: IdentityStatus) -> Self {
        match value {
            IdentityStatus::Unenrolled => Self::Unenrolled,
            IdentityStatus::Active => Self::Active,
            IdentityStatus::Retired => Self::Retired,
        }
    }
}

impl From<platform_application::installation_control::RehostStatus> for RehostStatusDto {
    fn from(value: platform_application::installation_control::RehostStatus) -> Self {
        use platform_application::installation_control::RehostStatus;
        match value {
            RehostStatus::Pending => Self::Pending,
            RehostStatus::Consumed => Self::Consumed,
            RehostStatus::Expired => Self::Expired,
            RehostStatus::Cancelled => Self::Cancelled,
        }
    }
}

fn parse_administrative_status(value: &str) -> Result<AdministrativeStatus, HttpError> {
    match value {
        "active" => Ok(AdministrativeStatus::Active),
        "suspended" => Ok(AdministrativeStatus::Suspended),
        "unregistered" => Ok(AdministrativeStatus::Unregistered),
        _ => Err(installation_error(InstallationControlError::InvalidInput)),
    }
}

fn parse_identity_status(value: &str) -> Result<IdentityStatus, HttpError> {
    match value {
        "unenrolled" => Ok(IdentityStatus::Unenrolled),
        "active" => Ok(IdentityStatus::Active),
        "retired" => Ok(IdentityStatus::Retired),
        _ => Err(installation_error(InstallationControlError::InvalidInput)),
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct InstallationOrganizationResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
    pub status: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct InstallationProductResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlInstallationSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub device_label: String,
    pub organization: InstallationOrganizationResponse,
    pub product: Option<InstallationProductResponse>,
    pub administrative_status: AdministrativeStatusDto,
    pub identity_status: IdentityStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub registered_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub enrolled_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_known_activity_at: Option<time::OffsetDateTime>,
    pub identity_generation: Option<u32>,
    pub binding_active: bool,
    pub fingerprint: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlInstallationListResponse {
    pub items: Vec<ControlInstallationSummaryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct InstallationBindingResponse {
    #[serde(with = "time::serde::rfc3339")]
    pub bound_at: time::OffsetDateTime,
    #[schema(value_type = String, format = Uuid)]
    pub approved_by_account_id: uuid::Uuid,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct InstallationRehostResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub enrollment_request_id: uuid::Uuid,
    pub status: RehostStatusDto,
    pub reason: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlInstallationDetailResponse {
    pub summary: ControlInstallationSummaryResponse,
    #[serde(with = "time::serde::rfc3339::option")]
    pub current_identity_created_at: Option<time::OffsetDateTime>,
    pub current_binding: Option<InstallationBindingResponse>,
    pub rehost: Option<InstallationRehostResponse>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RehostRequest {
    #[schema(value_type = String, format = Uuid)]
    pub enrollment_request_id: uuid::Uuid,
    pub reason: String,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InstallationListQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub status: Option<String>,
    pub identity_status: Option<String>,
    pub organization_id: Option<uuid::Uuid>,
    pub product_id: Option<uuid::Uuid>,
    pub search: Option<String>,
}

fn summary_response(value: InstallationSummary) -> ControlInstallationSummaryResponse {
    ControlInstallationSummaryResponse {
        id: value.id.as_uuid(),
        device_label: value.device_label,
        organization: InstallationOrganizationResponse {
            id: value.organization_id.as_uuid(),
            display_name: value.organization_display_name,
            status: value.organization_status,
        },
        product: value.product_id.map(|id| InstallationProductResponse {
            id: id.as_uuid(),
            key: value.product_key.unwrap_or_default(),
            name: value.product_name.unwrap_or_default(),
        }),
        administrative_status: value.administrative_status.into(),
        identity_status: value.identity_status.into(),
        registered_at: value.registered_at,
        enrolled_at: value.enrolled_at,
        last_known_activity_at: value.last_known_activity_at,
        identity_generation: value.identity_generation,
        binding_active: value.binding_active,
        fingerprint: value.fingerprint,
    }
}

fn detail_response(value: InstallationDetail) -> ControlInstallationDetailResponse {
    ControlInstallationDetailResponse {
        summary: summary_response(value.summary),
        current_identity_created_at: value.current_identity_created_at,
        current_binding: value
            .current_binding
            .map(|binding| InstallationBindingResponse {
                bound_at: binding.bound_at,
                approved_by_account_id: binding.approved_by_account_id.as_uuid(),
            }),
        rehost: value.rehost.map(rehost_response),
    }
}

fn rehost_response(value: RehostAuthorizationView) -> InstallationRehostResponse {
    InstallationRehostResponse {
        id: value.id,
        enrollment_request_id: value.enrollment_request_id.as_uuid(),
        status: value.status.into(),
        reason: value.reason,
        created_at: value.created_at,
        expires_at: value.expires_at,
    }
}

#[utoipa::path(get, path = "/api/v1/control/installations", summary = "Lista instalaciones POS", operation_id = "listControlInstallations", tag = "Control Center",
    security(("staffSession" = [])),
    params(InstallationListQuery),
    responses(
        (status = 200, description = "Instalaciones paginadas por alta e identificador", body = ControlInstallationListResponse),
        (status = 400, description = "Filtro, búsqueda o cursor inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn list_control_installations(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<InstallationListQuery>, QueryRejection>,
) -> Result<Json<ControlInstallationListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = InstallationFilters {
        status: query
            .status
            .as_deref()
            .map(parse_administrative_status)
            .transpose()?,
        identity_status: query
            .identity_status
            .as_deref()
            .map(parse_identity_status)
            .transpose()?,
        organization_id: query.organization_id.map(OrganizationId::from_uuid),
        product_id: query.product_id.map(ProductId::from_uuid),
        search: query.search,
    };
    let page = auth_state
        .staff
        .installations
        .list(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(installation_error)?;
    Ok(Json(ControlInstallationListResponse {
        items: page.items.into_iter().map(summary_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/installations/{installation_id}", summary = "Obtiene una instalación POS", operation_id = "getControlInstallation", tag = "Control Center",
    security(("staffSession" = [])),
    params(("installation_id" = String, Path, description = "UUID de la instalación")),
    responses(
        (status = 200, description = "Instalación con identidad y binding actuales", body = ControlInstallationDetailResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Instalación no encontrada", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn get_control_installation(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<ControlInstallationDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .installations
        .get(&actor, InstallationId::from_uuid(id))
        .await
        .map_err(installation_error)?;
    Ok(Json(detail_response(detail)))
}

#[utoipa::path(post, path = "/api/v1/control/installations/{installation_id}/suspend", summary = "Suspende administrativamente una instalación", operation_id = "suspendControlInstallation", tag = "Control Center",
    security(("staffSession" = [])),
    params(("installation_id" = String, Path, description = "UUID de la instalación")),
    responses(
        (status = 200, description = "Instalación suspendida o ya suspendida", body = ControlInstallationDetailResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Instalación no encontrada", body = ErrorResponse),
        (status = 409, description = "Transición no permitida", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn suspend_control_installation(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<ControlInstallationDetailResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .installations
        .suspend(
            &actor,
            InstallationId::from_uuid(id),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(installation_error)?;
    Ok(Json(detail_response(detail)))
}

#[utoipa::path(post, path = "/api/v1/control/installations/{installation_id}/reactivate", summary = "Reactiva administrativamente una instalación", operation_id = "reactivateControlInstallation", tag = "Control Center",
    security(("staffSession" = [])),
    params(("installation_id" = String, Path, description = "UUID de la instalación")),
    responses(
        (status = 200, description = "Instalación reactivada o ya activa", body = ControlInstallationDetailResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Instalación no encontrada", body = ErrorResponse),
        (status = 409, description = "Transición no permitida", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn reactivate_control_installation(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<ControlInstallationDetailResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .installations
        .reactivate(
            &actor,
            InstallationId::from_uuid(id),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(installation_error)?;
    Ok(Json(detail_response(detail)))
}

#[utoipa::path(post, path = "/api/v1/control/installations/{installation_id}/rehost", summary = "Autoriza el rehost de una instalación", operation_id = "rehostControlInstallation", tag = "Control Center",
    security(("staffSession" = [])),
    params(("installation_id" = String, Path, description = "UUID de la instalación")),
    request_body(content = RehostRequest),
    responses(
        (status = 200, description = "Autorización de rehost vigente o ya consumida", body = InstallationRehostResponse),
        (status = 400, description = "UUID o motivo inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Instalación no encontrada", body = ErrorResponse),
        (status = 409, description = "Conflicto de rehost, transición o producto", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn rehost_control_installation(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<uuid::Uuid>, PathRejection>,
    request: Result<Json<RehostRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<InstallationRehostResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(id) = path.map_err(path_error)?;
    let Json(request) =
        request.map_err(|_| installation_error(InstallationControlError::InvalidInput))?;
    let actor = actor(&auth_state, &context).await?;
    let authorization = auth_state
        .staff
        .installations
        .authorize_rehost(
            &actor,
            InstallationId::from_uuid(id),
            EnrollmentRequestId::from_uuid(request.enrollment_request_id),
            request.reason,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(installation_error)?;
    Ok(Json(rehost_response(authorization)))
}
