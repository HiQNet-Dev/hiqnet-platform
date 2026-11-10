//! Customer Portal comercial read-only (B7b).

use axum::{
    Json,
    extract::{Path, State, rejection::PathRejection},
    http::StatusCode,
};
use platform_application::commercial_domain::{
    CommercialError, Entitlements, PortalProduct, PortalSubscription, SubscriptionStatus,
};
use platform_core::ids::{OrganizationId, ProductId};
use std::collections::BTreeMap;

use crate::AppState;
use crate::auth::{self, AuthContext};
use crate::contract::ErrorResponse;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn portal_error(error: CommercialError) -> HttpError {
    match error {
        CommercialError::NotFound => {
            auth::error(StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        CommercialError::Forbidden => {
            auth::error(StatusCode::FORBIDDEN, "forbidden", "acceso insuficiente")
        }
        CommercialError::InvalidInput => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        CommercialError::Conflict | CommercialError::IdempotencyConflict => auth::error(
            StatusCode::CONFLICT,
            "conflict",
            "la solicitud entra en conflicto",
        ),
        CommercialError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
}

fn path_error(_rejection: PathRejection) -> HttpError {
    portal_error(CommercialError::InvalidInput)
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalProductRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalPlanRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
    pub is_trial: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalSubscriptionResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub status: String,
    pub plan: PortalPlanRefResponse,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub ends_at: time::OffsetDateTime,
    pub is_trial: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalEntitlementsResponse {
    pub effective: bool,
    pub status: Option<String>,
    pub reason: String,
    pub plan: Option<PortalPlanRefResponse>,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub starts_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ends_at: Option<time::OffsetDateTime>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalProductResponse {
    pub product: PortalProductRefResponse,
    pub organization_product_status: String,
    pub subscription: Option<PortalSubscriptionResponse>,
    pub entitlements: PortalEntitlementsResponse,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PortalProductListResponse {
    pub items: Vec<PortalProductResponse>,
}

fn plan_response(plan: platform_application::commercial_domain::PlanRef) -> PortalPlanRefResponse {
    PortalPlanRefResponse {
        id: plan.id,
        key: plan.key,
        name: plan.name,
        is_trial: plan.is_trial,
    }
}

fn entitlements_response(value: Entitlements) -> PortalEntitlementsResponse {
    PortalEntitlementsResponse {
        effective: value.effective,
        status: value
            .status
            .map(SubscriptionStatus::as_str)
            .map(str::to_owned),
        reason: value.reason,
        plan: value.plan.map(plan_response),
        features: value.features,
        limits: value.limits,
        starts_at: value.starts_at,
        ends_at: value.ends_at,
    }
}

fn subscription_response(value: PortalSubscription) -> PortalSubscriptionResponse {
    PortalSubscriptionResponse {
        id: value.id,
        status: value.status.as_str().to_owned(),
        plan: plan_response(value.plan),
        starts_at: value.starts_at,
        ends_at: value.ends_at,
        is_trial: value.is_trial,
    }
}

fn product_response(value: PortalProduct) -> PortalProductResponse {
    PortalProductResponse {
        product: PortalProductRefResponse {
            id: value.product.id.as_uuid(),
            key: value.product.key,
            name: value.product.name,
        },
        organization_product_status: value.organization_product_status,
        subscription: value.subscription.map(subscription_response),
        entitlements: entitlements_response(value.entitlements),
    }
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, HttpError> {
    uuid::Uuid::parse_str(value.trim()).map_err(|_| portal_error(CommercialError::InvalidInput))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/products", summary = "Lista productos contratados de la organización", operation_id = "listOrganizationProducts", tag = "Commercial Portal",
    security(("commercialSession" = [])),
    params(("organization_id" = String, Path, description = "UUID de la organización")),
    responses(
        (status = 200, description = "Productos contratados", body = PortalProductListResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión comercial requerida", body = ErrorResponse),
        (status = 404, description = "Organización ajena o inexistente (neutral)", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_organization_products(
    State(state): State<AppState>,
    auth_context: AuthContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<PortalProductListResponse>, HttpError> {
    let Path(organization_id) = path.map_err(path_error)?;
    let organization_id = OrganizationId::from_uuid(parse_uuid(&organization_id)?);
    let items = state
        .commercial_domain
        .portal_products(
            organization_id,
            auth_context.session.account_id,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(portal_error)?;
    Ok(Json(PortalProductListResponse {
        items: items.into_iter().map(product_response).collect(),
    }))
}

#[utoipa::path(get, path = "/api/v1/organizations/{organization_id}/products/{product_id}", summary = "Obtiene un producto contratado", operation_id = "getOrganizationProduct", tag = "Commercial Portal",
    security(("commercialSession" = [])),
    params(
        ("organization_id" = String, Path, description = "UUID de la organización"),
        ("product_id" = String, Path, description = "UUID del producto")
    ),
    responses(
        (status = 200, description = "Producto contratado", body = PortalProductResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión comercial requerida", body = ErrorResponse),
        (status = 404, description = "Recurso ajeno o inexistente (neutral)", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_organization_product(
    State(state): State<AppState>,
    auth_context: AuthContext,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<Json<PortalProductResponse>, HttpError> {
    let Path((organization_id, product_id)) = path.map_err(path_error)?;
    let product = state
        .commercial_domain
        .portal_product(
            OrganizationId::from_uuid(parse_uuid(&organization_id)?),
            ProductId::from_uuid(parse_uuid(&product_id)?),
            auth_context.session.account_id,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(portal_error)?;
    Ok(Json(product_response(product)))
}
