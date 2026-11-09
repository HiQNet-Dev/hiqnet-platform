//! Vistas globales read-only de Organizations y Accounts para el Control Center.
//!
//! Exclusivamente GET. Autoriza por capability staff antes de consultar
//! persistencia y no reutiliza endpoints ni autoridad comercial.

use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{PathRejection, QueryRejection},
    },
    http::StatusCode,
};
use platform_application::control::{
    AccountStatus, AssociationStatus, ControlAccountDetail, ControlActor, ControlError,
    ControlMembershipView, ControlOrganizationDetail, MembershipRole, OrganizationStatus,
};
use platform_core::{
    ids::{CommercialAccountId, OrganizationId},
    product::LifecycleStatus,
};

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn control_error(error: ControlError) -> HttpError {
    match error {
        ControlError::NotFound => {
            auth::error(StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        ControlError::Forbidden => auth::error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        ControlError::InvalidInput => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        ControlError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
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
pub enum OrganizationStatusDto {
    Active,
    LegacyUnclaimed,
    Archived,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatusDto {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRoleDto {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssociationStatusDto {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProductLifecycleDto {
    Development,
    Active,
    Maintenance,
    Deprecated,
    Retired,
}

impl From<OrganizationStatus> for OrganizationStatusDto {
    fn from(value: OrganizationStatus) -> Self {
        match value {
            OrganizationStatus::Active => Self::Active,
            OrganizationStatus::LegacyUnclaimed => Self::LegacyUnclaimed,
            OrganizationStatus::Archived => Self::Archived,
        }
    }
}

impl From<AccountStatus> for AccountStatusDto {
    fn from(value: AccountStatus) -> Self {
        match value {
            AccountStatus::Active => Self::Active,
            AccountStatus::Disabled => Self::Disabled,
        }
    }
}

impl From<MembershipRole> for MembershipRoleDto {
    fn from(value: MembershipRole) -> Self {
        match value {
            MembershipRole::Owner => Self::Owner,
            MembershipRole::Admin => Self::Admin,
            MembershipRole::Member => Self::Member,
        }
    }
}

impl From<AssociationStatus> for AssociationStatusDto {
    fn from(value: AssociationStatus) -> Self {
        match value {
            AssociationStatus::Active => Self::Active,
            AssociationStatus::Inactive => Self::Inactive,
        }
    }
}

impl From<LifecycleStatus> for ProductLifecycleDto {
    fn from(value: LifecycleStatus) -> Self {
        match value {
            LifecycleStatus::Development => Self::Development,
            LifecycleStatus::Active => Self::Active,
            LifecycleStatus::Maintenance => Self::Maintenance,
            LifecycleStatus::Deprecated => Self::Deprecated,
            LifecycleStatus::Retired => Self::Retired,
        }
    }
}

fn parse_organization_status(value: &str) -> Result<OrganizationStatus, HttpError> {
    match value {
        "active" => Ok(OrganizationStatus::Active),
        "legacy_unclaimed" => Ok(OrganizationStatus::LegacyUnclaimed),
        "archived" => Ok(OrganizationStatus::Archived),
        _ => Err(control_error(ControlError::InvalidInput)),
    }
}

fn parse_account_status(value: &str) -> Result<AccountStatus, HttpError> {
    match value {
        "active" => Ok(AccountStatus::Active),
        "disabled" => Ok(AccountStatus::Disabled),
        _ => Err(control_error(ControlError::InvalidInput)),
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
    pub status: OrganizationStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationListResponse {
    pub items: Vec<ControlOrganizationSummaryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlMembershipResponse {
    #[schema(value_type = String, format = Uuid)]
    pub account_id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
    pub account_status: AccountStatusDto,
    pub role: MembershipRoleDto,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<time::OffsetDateTime>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationProductResponse {
    #[schema(value_type = String, format = Uuid)]
    pub product_id: uuid::Uuid,
    pub product_key: String,
    pub product_name: String,
    pub product_status: ProductLifecycleDto,
    pub association_status: AssociationStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub activated_at: time::OffsetDateTime,
    #[schema(value_type = Object)]
    pub entitlements: serde_json::Value,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub display_name: String,
    pub status: OrganizationStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    pub memberships: Vec<ControlMembershipResponse>,
    pub products: Vec<ControlOrganizationProductResponse>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAccountSummaryResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
    pub status: AccountStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAccountListResponse {
    pub items: Vec<ControlAccountSummaryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAccountMembershipResponse {
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: uuid::Uuid,
    pub organization_display_name: String,
    pub organization_status: OrganizationStatusDto,
    pub role: MembershipRoleDto,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<time::OffsetDateTime>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlAccountDetailResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
    pub status: AccountStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    pub memberships: Vec<ControlAccountMembershipResponse>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ControlDirectoryQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
}

fn organization_response(
    value: platform_application::control::ControlOrganizationSummary,
) -> ControlOrganizationSummaryResponse {
    ControlOrganizationSummaryResponse {
        id: value.id.as_uuid(),
        display_name: value.display_name,
        status: value.status.into(),
        created_at: value.created_at,
    }
}

fn membership_response(value: ControlMembershipView) -> ControlMembershipResponse {
    ControlMembershipResponse {
        account_id: value.account_id.as_uuid(),
        email: value.email,
        display_name: value.display_name,
        account_status: value.account_status.into(),
        role: value.role.into(),
        joined_at: value.joined_at,
        revoked_at: value.revoked_at,
    }
}

fn organization_detail_response(
    value: ControlOrganizationDetail,
) -> ControlOrganizationDetailResponse {
    ControlOrganizationDetailResponse {
        id: value.organization.id.as_uuid(),
        display_name: value.organization.display_name,
        status: value.organization.status.into(),
        created_at: value.organization.created_at,
        memberships: value
            .memberships
            .into_iter()
            .map(membership_response)
            .collect(),
        products: value
            .products
            .into_iter()
            .map(|product| ControlOrganizationProductResponse {
                product_id: product.product_id.as_uuid(),
                product_key: product.product_key,
                product_name: product.product_name,
                product_status: product.product_status.into(),
                association_status: product.association_status.into(),
                activated_at: product.activated_at,
                entitlements: product.entitlements,
            })
            .collect(),
    }
}

fn account_response(
    value: platform_application::control::ControlAccountSummary,
) -> ControlAccountSummaryResponse {
    ControlAccountSummaryResponse {
        id: value.id.as_uuid(),
        email: value.email,
        display_name: value.display_name,
        status: value.status.into(),
        created_at: value.created_at,
    }
}

fn account_detail_response(value: ControlAccountDetail) -> ControlAccountDetailResponse {
    ControlAccountDetailResponse {
        id: value.account.id.as_uuid(),
        email: value.account.email,
        display_name: value.account.display_name,
        status: value.account.status.into(),
        created_at: value.account.created_at,
        memberships: value
            .memberships
            .into_iter()
            .map(|membership| ControlAccountMembershipResponse {
                organization_id: membership.organization_id.as_uuid(),
                organization_display_name: membership.organization_display_name,
                organization_status: membership.organization_status.into(),
                role: membership.role.into(),
                joined_at: membership.joined_at,
                revoked_at: membership.revoked_at,
            })
            .collect(),
    }
}

#[utoipa::path(get, path = "/api/v1/control/organizations", summary = "Lista global de organizaciones", operation_id = "listControlOrganizations", tag = "Control Center",
    security(("staffSession" = [])),
    params(ControlDirectoryQuery),
    responses(
        (status = 200, description = "Organizaciones paginadas por antigüedad e identificador", body = ControlOrganizationListResponse),
        (status = 400, description = "Filtro, búsqueda o cursor inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn list_control_organizations(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<ControlDirectoryQuery>, QueryRejection>,
) -> Result<Json<ControlOrganizationListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let status = query
        .status
        .as_deref()
        .map(parse_organization_status)
        .transpose()?;
    let page = auth_state
        .staff
        .control
        .list_organizations(&actor, status, query.search, query.limit, query.cursor)
        .await
        .map_err(control_error)?;
    Ok(Json(ControlOrganizationListResponse {
        items: page.items.into_iter().map(organization_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/organizations/{organization_id}", summary = "Obtiene una organización global", operation_id = "getControlOrganization", tag = "Control Center",
    security(("staffSession" = [])),
    params(("organization_id" = String, Path, description = "UUID de la organización")),
    responses(
        (status = 200, description = "Organización con memberships y products", body = ControlOrganizationDetailResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Organización no encontrada", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn get_control_organization(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<ControlOrganizationDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let organization = auth_state
        .staff
        .control
        .get_organization(&actor, OrganizationId::from_uuid(id))
        .await
        .map_err(control_error)?;
    Ok(Json(organization_detail_response(organization)))
}

#[utoipa::path(get, path = "/api/v1/control/accounts", summary = "Lista global de cuentas comerciales", operation_id = "listControlAccounts", tag = "Control Center",
    security(("staffSession" = [])),
    params(ControlDirectoryQuery),
    responses(
        (status = 200, description = "Cuentas paginadas por antigüedad e identificador", body = ControlAccountListResponse),
        (status = 400, description = "Filtro, búsqueda o cursor inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn list_control_accounts(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<ControlDirectoryQuery>, QueryRejection>,
) -> Result<Json<ControlAccountListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let status = query
        .status
        .as_deref()
        .map(parse_account_status)
        .transpose()?;
    let page = auth_state
        .staff
        .control
        .list_accounts(&actor, status, query.search, query.limit, query.cursor)
        .await
        .map_err(control_error)?;
    Ok(Json(ControlAccountListResponse {
        items: page.items.into_iter().map(account_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/accounts/{account_id}", summary = "Obtiene una cuenta comercial global", operation_id = "getControlAccount", tag = "Control Center",
    security(("staffSession" = [])),
    params(("account_id" = String, Path, description = "UUID de la cuenta comercial")),
    responses(
        (status = 200, description = "Cuenta con sus memberships históricas", body = ControlAccountDetailResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Cuenta no encontrada", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn get_control_account(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<uuid::Uuid>, PathRejection>,
) -> Result<Json<ControlAccountDetailResponse>, HttpError> {
    let Path(id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let account = auth_state
        .staff
        .control
        .get_account(&actor, CommercialAccountId::from_uuid(id))
        .await
        .map_err(control_error)?;
    Ok(Json(account_detail_response(account)))
}
