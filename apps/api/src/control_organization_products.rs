//! Mutaciones administrativas de OrganizationProduct (B4).

use axum::http::HeaderMap;
use axum::{
    Extension, Json,
    extract::{
        Path,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
};
use platform_application::{
    control::AssociationStatus,
    organization_product::{
        OrganizationProductActor, OrganizationProductError, OrganizationProductMutation,
    },
};
use platform_core::ids::{OrganizationId, ProductId};

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::control_directory::{
    AssociationStatusDto, ControlOrganizationProductResponse, ProductLifecycleDto,
};
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn product_error(error: OrganizationProductError) -> HttpError {
    let (status, code, message) = match error {
        OrganizationProductError::NotFound => {
            (StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        OrganizationProductError::Forbidden => (
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        OrganizationProductError::InvalidInput => (
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        OrganizationProductError::Storage => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    };
    auth::error(status, code, message)
}

fn path_error(_rejection: PathRejection) -> HttpError {
    product_error(OrganizationProductError::InvalidInput)
}

fn body_error(_rejection: JsonRejection) -> HttpError {
    product_error(OrganizationProductError::InvalidInput)
}

async fn actor(
    auth_state: &AuthState,
    context: &StaffContext,
) -> Result<OrganizationProductActor, HttpError> {
    let identity = auth_state
        .staff
        .service
        .identity(context.session.account_id)
        .await
        .map_err(crate::staff::staff_error)?;
    Ok(OrganizationProductActor::new(
        identity.account_id,
        identity.role,
    ))
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, HttpError> {
    uuid::Uuid::parse_str(value.trim())
        .map_err(|_| product_error(OrganizationProductError::InvalidInput))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationProductListResponse {
    pub items: Vec<ControlOrganizationProductResponse>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ControlOrganizationProductMutationResponse {
    pub product: ControlOrganizationProductResponse,
    pub changed: bool,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ActivateOrganizationProductRequest {
    #[schema(value_type = String, format = Uuid)]
    pub product_id: uuid::Uuid,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct UpdateOrganizationProductRequest {
    pub status: AssociationStatusDto,
}

fn product_response(
    value: platform_application::control::ControlOrganizationProductView,
) -> ControlOrganizationProductResponse {
    ControlOrganizationProductResponse {
        product_id: value.product_id.as_uuid(),
        product_key: value.product_key,
        product_name: value.product_name,
        product_status: ProductLifecycleDto::from(value.product_status),
        association_status: AssociationStatusDto::from(value.association_status),
        activated_at: value.activated_at,
        entitlements: value.entitlements,
    }
}

fn mutation_response(
    value: OrganizationProductMutation,
) -> ControlOrganizationProductMutationResponse {
    ControlOrganizationProductMutationResponse {
        product: product_response(value.view),
        changed: value.changed,
    }
}

#[utoipa::path(get, path = "/api/v1/control/organizations/{organization_id}/products", summary = "Lista productos asociados a una organización", operation_id = "listControlOrganizationProducts", tag = "Control Center",
    security(("staffSession" = [])),
    params(("organization_id" = String, Path, description = "UUID de la organización")),
    responses(
        (status = 200, description = "Asociaciones de producto", body = ControlOrganizationProductListResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Organización no encontrada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_organization_products(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ControlOrganizationProductListResponse>, HttpError> {
    let Path(organization_id) = path.map_err(path_error)?;
    let organization_id = OrganizationId::from_uuid(parse_uuid(&organization_id)?);
    let actor = actor(&auth_state, &context).await?;
    let items = auth_state
        .staff
        .organization_products
        .list(&actor, organization_id)
        .await
        .map_err(product_error)?;
    Ok(Json(ControlOrganizationProductListResponse {
        items: items.into_iter().map(product_response).collect(),
    }))
}

#[utoipa::path(post, path = "/api/v1/control/organizations/{organization_id}/products", summary = "Activa una asociación organización-producto", operation_id = "activateControlOrganizationProduct", tag = "Control Center",
    security(("staffSession" = [])),
    params(("organization_id" = String, Path, description = "UUID de la organización")),
    request_body = ActivateOrganizationProductRequest,
    responses(
        (status = 200, description = "Asociación activa", body = ControlOrganizationProductMutationResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Organización o producto no encontrado", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn activate_control_organization_product(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<ActivateOrganizationProductRequest>, JsonRejection>,
) -> Result<Json<ControlOrganizationProductMutationResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(organization_id) = path.map_err(path_error)?;
    let Json(body) = body.map_err(body_error)?;
    let organization_id = OrganizationId::from_uuid(parse_uuid(&organization_id)?);
    let product_id = ProductId::from_uuid(body.product_id);
    let actor = actor(&auth_state, &context).await?;
    let mutation = auth_state
        .staff
        .organization_products
        .activate(
            &actor,
            organization_id,
            product_id,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(product_error)?;
    Ok(Json(mutation_response(mutation)))
}

#[utoipa::path(patch, path = "/api/v1/control/organizations/{organization_id}/products/{product_id}", summary = "Activa o desactiva una asociación organización-producto", operation_id = "updateControlOrganizationProduct", tag = "Control Center",
    security(("staffSession" = [])),
    params(
        ("organization_id" = String, Path, description = "UUID de la organización"),
        ("product_id" = String, Path, description = "UUID del producto")
    ),
    request_body = UpdateOrganizationProductRequest,
    responses(
        (status = 200, description = "Asociación actualizada", body = ControlOrganizationProductMutationResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Asociación no encontrada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn update_control_organization_product(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    body: Result<Json<UpdateOrganizationProductRequest>, JsonRejection>,
) -> Result<Json<ControlOrganizationProductMutationResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path((organization_id, product_id)) = path.map_err(path_error)?;
    let Json(body) = body.map_err(body_error)?;
    let organization_id = OrganizationId::from_uuid(parse_uuid(&organization_id)?);
    let product_id = ProductId::from_uuid(parse_uuid(&product_id)?);
    let status = match body.status {
        AssociationStatusDto::Active => AssociationStatus::Active,
        AssociationStatusDto::Inactive => AssociationStatus::Inactive,
    };
    let actor = actor(&auth_state, &context).await?;
    let mutation = auth_state
        .staff
        .organization_products
        .set_status(
            &actor,
            organization_id,
            product_id,
            status,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(product_error)?;
    Ok(Json(mutation_response(mutation)))
}
