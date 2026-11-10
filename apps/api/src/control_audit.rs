//! Audit y Security Center read-only (C6.1).

use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{PathRejection, QueryRejection},
    },
    http::StatusCode,
};
use platform_application::audit_security::{
    AuditEventDetail, AuditEventSummary, AuditFilters, AuditResult, AuditSecurityActor,
    AuditSecurityError, SecurityEventDetail, SecurityEventSummary, SecurityFilters, SecurityKind,
    StaffAuthAction, StaffAuthEventDetail, StaffAuthEventSummary, StaffAuthFilters,
};
use platform_core::ids::{CommercialAccountId, OrganizationId, ProductId, StaffAccountId};
use serde_json::Value;
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn audit_error(error: AuditSecurityError) -> HttpError {
    let (status, code, message) = match error {
        AuditSecurityError::NotFound => {
            (StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        AuditSecurityError::Forbidden => (
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        AuditSecurityError::InvalidInput => (
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        AuditSecurityError::Storage => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    };
    auth::error(status, code, message)
}

fn query_error(_rejection: QueryRejection) -> HttpError {
    audit_error(AuditSecurityError::InvalidInput)
}

fn path_error(_rejection: PathRejection) -> HttpError {
    audit_error(AuditSecurityError::InvalidInput)
}

async fn actor(
    auth_state: &AuthState,
    context: &StaffContext,
) -> Result<AuditSecurityActor, HttpError> {
    let identity = auth_state
        .staff
        .service
        .identity(context.session.account_id)
        .await
        .map_err(crate::staff::staff_error)?;
    Ok(AuditSecurityActor::new(identity.account_id, identity.role))
}

fn parse_uuid(value: Option<&str>) -> Result<Option<uuid::Uuid>, HttpError> {
    value
        .map(|value| {
            uuid::Uuid::parse_str(value.trim())
                .map_err(|_| audit_error(AuditSecurityError::InvalidInput))
        })
        .transpose()
}

fn parse_time(value: Option<&str>) -> Result<Option<time::OffsetDateTime>, HttpError> {
    value
        .map(|value| {
            time::OffsetDateTime::parse(value.trim(), &Rfc3339)
                .map_err(|_| audit_error(AuditSecurityError::InvalidInput))
        })
        .transpose()
}

fn parse_audit_result(value: Option<&str>) -> Result<Option<AuditResult>, HttpError> {
    value
        .map(|value| {
            AuditResult::from_str(value).map_err(|_| audit_error(AuditSecurityError::InvalidInput))
        })
        .transpose()
}

fn parse_security_kind(value: Option<&str>) -> Result<Option<SecurityKind>, HttpError> {
    value
        .map(|value| {
            SecurityKind::from_str(value).map_err(|_| audit_error(AuditSecurityError::InvalidInput))
        })
        .transpose()
}

fn parse_staff_auth_action(value: Option<&str>) -> Result<Option<StaffAuthAction>, HttpError> {
    value
        .map(|value| {
            StaffAuthAction::from_str(value)
                .map_err(|_| audit_error(AuditSecurityError::InvalidInput))
        })
        .transpose()
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditResultDto {
    Success,
    Denied,
    Failure,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecurityKindDto {
    EnrollmentConflict,
    DuplicateKey,
    TenantChangeAttempt,
    ReplayRejected,
    RehostPerformed,
    IdentityRevoked,
    AuthorizationRevokedDuringProof,
    IdempotencyConflict,
    CloneSuspected,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaffAuthActionDto {
    LoginSuccess,
    LoginFailure,
    Logout,
    SessionRevoked,
    AccountLocked,
    PasswordChanged,
}

impl From<AuditResult> for AuditResultDto {
    fn from(value: AuditResult) -> Self {
        match value {
            AuditResult::Success => Self::Success,
            AuditResult::Denied => Self::Denied,
            AuditResult::Failure => Self::Failure,
        }
    }
}

impl From<SecurityKind> for SecurityKindDto {
    fn from(value: SecurityKind) -> Self {
        match value {
            SecurityKind::EnrollmentConflict => Self::EnrollmentConflict,
            SecurityKind::DuplicateKey => Self::DuplicateKey,
            SecurityKind::TenantChangeAttempt => Self::TenantChangeAttempt,
            SecurityKind::ReplayRejected => Self::ReplayRejected,
            SecurityKind::RehostPerformed => Self::RehostPerformed,
            SecurityKind::IdentityRevoked => Self::IdentityRevoked,
            SecurityKind::AuthorizationRevokedDuringProof => Self::AuthorizationRevokedDuringProof,
            SecurityKind::IdempotencyConflict => Self::IdempotencyConflict,
            SecurityKind::CloneSuspected => Self::CloneSuspected,
        }
    }
}

impl From<StaffAuthAction> for StaffAuthActionDto {
    fn from(value: StaffAuthAction) -> Self {
        match value {
            StaffAuthAction::LoginSuccess => Self::LoginSuccess,
            StaffAuthAction::LoginFailure => Self::LoginFailure,
            StaffAuthAction::Logout => Self::Logout,
            StaffAuthAction::SessionRevoked => Self::SessionRevoked,
            StaffAuthAction::AccountLocked => Self::AccountLocked,
            StaffAuthAction::PasswordChanged => Self::PasswordChanged,
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlStaffActorResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub display_name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlStaffActorDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub display_name: String,
    pub email: String,
    pub status: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlProductRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlCommercialAccountRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlCommercialAccountDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
    pub email: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlInstallationRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub device_label: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAuditEventSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub actor: Option<ControlStaffActorResponse>,
    pub action: String,
    pub resource_type: String,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub resource_id: Option<uuid::Uuid>,
    pub resource_label: Option<String>,
    pub organization: Option<ControlOrganizationRefResponse>,
    pub product: Option<ControlProductRefResponse>,
    pub result: AuditResultDto,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAuditEventDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub actor: Option<ControlStaffActorDetailResponse>,
    pub action: String,
    pub resource_type: String,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub resource_id: Option<uuid::Uuid>,
    pub resource_label: Option<String>,
    pub organization: Option<ControlOrganizationRefResponse>,
    pub product: Option<ControlProductRefResponse>,
    pub result: AuditResultDto,
    pub metadata: Value,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAuditEventListResponse {
    pub items: Vec<ControlAuditEventSummaryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlSecurityEventSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub kind: SecurityKindDto,
    pub organization: Option<ControlOrganizationRefResponse>,
    pub installation: Option<ControlInstallationRefResponse>,
    pub account: Option<ControlCommercialAccountRefResponse>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlSecurityEventDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub kind: SecurityKindDto,
    pub organization: Option<ControlOrganizationRefResponse>,
    pub installation: Option<ControlInstallationRefResponse>,
    pub account: Option<ControlCommercialAccountDetailResponse>,
    pub metadata: Value,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlSecurityEventListResponse {
    pub items: Vec<ControlSecurityEventSummaryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlStaffAuthEventSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub action: StaffAuthActionDto,
    pub account: Option<ControlStaffActorResponse>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlStaffAuthEventDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub action: StaffAuthActionDto,
    pub account: Option<ControlStaffActorDetailResponse>,
    pub metadata: Value,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlStaffAuthEventListResponse {
    pub items: Vec<ControlStaffAuthEventSummaryResponse>,
    pub next_cursor: Option<String>,
}

fn organization(
    value: platform_application::audit_security::OrganizationRef,
) -> ControlOrganizationRefResponse {
    ControlOrganizationRefResponse {
        id: value.id.as_uuid(),
        display_name: value.display_name,
    }
}

fn product(value: platform_application::audit_security::ProductRef) -> ControlProductRefResponse {
    ControlProductRefResponse {
        id: value.id.as_uuid(),
        key: value.key,
        name: value.name,
    }
}

fn staff_actor(
    value: platform_application::audit_security::StaffActorRef,
) -> ControlStaffActorResponse {
    ControlStaffActorResponse {
        account_id: value.account_id.as_uuid(),
        display_name: value.display_name,
    }
}

fn staff_actor_detail(
    value: platform_application::audit_security::StaffActorDetail,
) -> ControlStaffActorDetailResponse {
    ControlStaffActorDetailResponse {
        account_id: value.account_id.as_uuid(),
        display_name: value.display_name,
        email: value.email,
        status: value.status,
    }
}

fn commercial_account(
    value: platform_application::audit_security::CommercialAccountRef,
) -> ControlCommercialAccountRefResponse {
    ControlCommercialAccountRefResponse {
        id: value.id.as_uuid(),
        display_name: value.display_name,
    }
}

fn commercial_account_detail(
    value: platform_application::audit_security::CommercialAccountDetail,
) -> ControlCommercialAccountDetailResponse {
    ControlCommercialAccountDetailResponse {
        id: value.id.as_uuid(),
        display_name: value.display_name,
        email: value.email,
    }
}

fn installation(
    value: platform_application::audit_security::InstallationRef,
) -> ControlInstallationRefResponse {
    ControlInstallationRefResponse {
        id: value.id.as_uuid(),
        device_label: value.device_label,
    }
}

fn audit_summary(value: AuditEventSummary) -> ControlAuditEventSummaryResponse {
    ControlAuditEventSummaryResponse {
        id: value.id,
        occurred_at: value.occurred_at,
        actor: value.actor.map(staff_actor),
        action: value.action,
        resource_type: value.resource_type,
        resource_id: value.resource_id,
        resource_label: value.resource_label,
        organization: value.organization.map(organization),
        product: value.product.map(product),
        result: value.result.into(),
    }
}

fn audit_detail(value: AuditEventDetail) -> ControlAuditEventDetailResponse {
    ControlAuditEventDetailResponse {
        id: value.summary.id,
        occurred_at: value.summary.occurred_at,
        actor: value.actor.map(staff_actor_detail),
        action: value.summary.action,
        resource_type: value.summary.resource_type,
        resource_id: value.summary.resource_id,
        resource_label: value.summary.resource_label,
        organization: value.summary.organization.map(organization),
        product: value.summary.product.map(product),
        result: value.summary.result.into(),
        metadata: value.metadata,
    }
}

fn security_summary(value: SecurityEventSummary) -> ControlSecurityEventSummaryResponse {
    ControlSecurityEventSummaryResponse {
        id: value.id,
        occurred_at: value.occurred_at,
        kind: value.kind.into(),
        organization: value.organization.map(organization),
        installation: value.installation.map(installation),
        account: value.account.map(commercial_account),
    }
}

fn security_detail(value: SecurityEventDetail) -> ControlSecurityEventDetailResponse {
    ControlSecurityEventDetailResponse {
        id: value.summary.id,
        occurred_at: value.summary.occurred_at,
        kind: value.summary.kind.into(),
        organization: value.summary.organization.map(organization),
        installation: value.summary.installation.map(installation),
        account: value.account.map(commercial_account_detail),
        metadata: value.metadata,
    }
}

fn staff_auth_summary(value: StaffAuthEventSummary) -> ControlStaffAuthEventSummaryResponse {
    ControlStaffAuthEventSummaryResponse {
        id: value.id,
        occurred_at: value.occurred_at,
        action: value.action.into(),
        account: value.account.map(staff_actor),
    }
}

fn staff_auth_detail(value: StaffAuthEventDetail) -> ControlStaffAuthEventDetailResponse {
    ControlStaffAuthEventDetailResponse {
        id: value.summary.id,
        occurred_at: value.summary.occurred_at,
        action: value.summary.action.into(),
        account: value.account.map(staff_actor_detail),
        metadata: value.metadata,
    }
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditListQuery {
    pub staff_account_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub organization_id: Option<String>,
    pub product_id: Option<String>,
    pub result: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SecurityListQuery {
    pub kind: Option<String>,
    pub organization_id: Option<String>,
    pub installation_id: Option<String>,
    pub account_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StaffAuthListQuery {
    pub account_id: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/control/audit", summary = "Lista eventos de auditoría staff", operation_id = "listControlAudit", tag = "Control Center",
    security(("staffSession" = [])),
    params(AuditListQuery),
    responses(
        (status = 200, description = "Eventos de auditoría", body = ControlAuditEventListResponse),
        (status = 400, description = "Consulta inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_audit(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<AuditListQuery>, QueryRejection>,
) -> Result<Json<ControlAuditEventListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = AuditFilters {
        staff_account_id: parse_uuid(query.staff_account_id.as_deref())?
            .map(StaffAccountId::from_uuid),
        action: query.action,
        resource_type: query.resource_type,
        resource_id: parse_uuid(query.resource_id.as_deref())?,
        organization_id: parse_uuid(query.organization_id.as_deref())?
            .map(OrganizationId::from_uuid),
        product_id: parse_uuid(query.product_id.as_deref())?.map(ProductId::from_uuid),
        result: parse_audit_result(query.result.as_deref())?,
        from: parse_time(query.from.as_deref())?,
        to: parse_time(query.to.as_deref())?,
    };
    let page = auth_state
        .staff
        .audit_security
        .list_audit(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(audit_error)?;
    Ok(Json(ControlAuditEventListResponse {
        items: page.items.into_iter().map(audit_summary).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/audit/{event_id}", summary = "Obtiene un evento de auditoría staff", operation_id = "getControlAudit", tag = "Control Center",
    security(("staffSession" = [])),
    params(("event_id" = String, Path, description = "UUID del evento")),
    responses(
        (status = 200, description = "Evento de auditoría", body = ControlAuditEventDetailResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Recurso no encontrado", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_audit(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ControlAuditEventDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let id = parse_uuid(Some(&id))?.ok_or_else(|| audit_error(AuditSecurityError::InvalidInput))?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .audit_security
        .get_audit(&actor, id)
        .await
        .map_err(audit_error)?;
    Ok(Json(audit_detail(detail)))
}

#[utoipa::path(get, path = "/api/v1/control/security-events", summary = "Lista eventos de seguridad de plataforma", operation_id = "listControlSecurityEvents", tag = "Control Center",
    security(("staffSession" = [])),
    params(SecurityListQuery),
    responses(
        (status = 200, description = "Eventos de seguridad", body = ControlSecurityEventListResponse),
        (status = 400, description = "Consulta inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_security_events(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<SecurityListQuery>, QueryRejection>,
) -> Result<Json<ControlSecurityEventListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = SecurityFilters {
        kind: parse_security_kind(query.kind.as_deref())?,
        organization_id: parse_uuid(query.organization_id.as_deref())?
            .map(OrganizationId::from_uuid),
        installation_id: parse_uuid(query.installation_id.as_deref())?
            .map(platform_core::ids::InstallationId::from_uuid),
        account_id: parse_uuid(query.account_id.as_deref())?.map(CommercialAccountId::from_uuid),
        from: parse_time(query.from.as_deref())?,
        to: parse_time(query.to.as_deref())?,
    };
    let page = auth_state
        .staff
        .audit_security
        .list_security(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(audit_error)?;
    Ok(Json(ControlSecurityEventListResponse {
        items: page.items.into_iter().map(security_summary).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/security-events/{event_id}", summary = "Obtiene un evento de seguridad de plataforma", operation_id = "getControlSecurityEvent", tag = "Control Center",
    security(("staffSession" = [])),
    params(("event_id" = String, Path, description = "UUID del evento")),
    responses(
        (status = 200, description = "Evento de seguridad", body = ControlSecurityEventDetailResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Recurso no encontrado", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_security_event(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ControlSecurityEventDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let id = parse_uuid(Some(&id))?.ok_or_else(|| audit_error(AuditSecurityError::InvalidInput))?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .audit_security
        .get_security(&actor, id)
        .await
        .map_err(audit_error)?;
    Ok(Json(security_detail(detail)))
}

#[utoipa::path(get, path = "/api/v1/control/staff-auth-events", summary = "Lista eventos de autenticación staff", operation_id = "listControlStaffAuthEvents", tag = "Control Center",
    security(("staffSession" = [])),
    params(StaffAuthListQuery),
    responses(
        (status = 200, description = "Eventos de autenticación staff", body = ControlStaffAuthEventListResponse),
        (status = 400, description = "Consulta inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_staff_auth_events(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<StaffAuthListQuery>, QueryRejection>,
) -> Result<Json<ControlStaffAuthEventListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = StaffAuthFilters {
        account_id: parse_uuid(query.account_id.as_deref())?.map(StaffAccountId::from_uuid),
        action: parse_staff_auth_action(query.action.as_deref())?,
        from: parse_time(query.from.as_deref())?,
        to: parse_time(query.to.as_deref())?,
    };
    let page = auth_state
        .staff
        .audit_security
        .list_staff_auth(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(audit_error)?;
    Ok(Json(ControlStaffAuthEventListResponse {
        items: page.items.into_iter().map(staff_auth_summary).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/staff-auth-events/{event_id}", summary = "Obtiene un evento de autenticación staff", operation_id = "getControlStaffAuthEvent", tag = "Control Center",
    security(("staffSession" = [])),
    params(("event_id" = String, Path, description = "UUID del evento")),
    responses(
        (status = 200, description = "Evento de autenticación staff", body = ControlStaffAuthEventDetailResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Recurso no encontrado", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_staff_auth_event(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ControlStaffAuthEventDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let id = parse_uuid(Some(&id))?.ok_or_else(|| audit_error(AuditSecurityError::InvalidInput))?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .audit_security
        .get_staff_auth(&actor, id)
        .await
        .map_err(audit_error)?;
    Ok(Json(staff_auth_detail(detail)))
}
