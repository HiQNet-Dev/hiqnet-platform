//! Commercial Core HTTP: planes, suscripciones y entitlements (B6).

use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use platform_application::commercial_domain::{
    CommercialActor, CommercialError, CreateSubscription, Entitlements, NewPlan, Plan, PlanFilters,
    PlanPatch, PlanStatus, RenewSubscription, Subscription, SubscriptionAction, SubscriptionDetail,
    SubscriptionEvent, SubscriptionFilters, SubscriptionStatus, SubscriptionTransition,
};
use platform_core::ids::{OrganizationId, ProductId};
use std::collections::BTreeMap;
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn commercial_error(error: CommercialError) -> HttpError {
    let (status, code, message) = match error {
        CommercialError::NotFound => (StatusCode::NOT_FOUND, "not_found", "recurso no encontrado"),
        CommercialError::Forbidden => (
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        CommercialError::InvalidInput => (
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        CommercialError::Conflict => (
            StatusCode::CONFLICT,
            "conflict",
            "la operación entra en conflicto con el estado actual",
        ),
        CommercialError::IdempotencyConflict => (
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "la clave de idempotencia ya fue usada con otro payload",
        ),
        CommercialError::Storage => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    };
    auth::error(status, code, message)
}

fn query_error(_rejection: QueryRejection) -> HttpError {
    commercial_error(CommercialError::InvalidInput)
}

fn path_error(_rejection: PathRejection) -> HttpError {
    commercial_error(CommercialError::InvalidInput)
}

fn body_error(_rejection: JsonRejection) -> HttpError {
    commercial_error(CommercialError::InvalidInput)
}

async fn actor(
    auth_state: &AuthState,
    context: &StaffContext,
) -> Result<CommercialActor, HttpError> {
    let identity = auth_state
        .staff
        .service
        .identity(context.session.account_id)
        .await
        .map_err(crate::staff::staff_error)?;
    Ok(CommercialActor::new(identity.account_id, identity.role))
}

fn parse_uuid(value: &str) -> Result<uuid::Uuid, HttpError> {
    uuid::Uuid::parse_str(value.trim()).map_err(|_| commercial_error(CommercialError::InvalidInput))
}

fn parse_time(value: &str) -> Result<time::OffsetDateTime, HttpError> {
    time::OffsetDateTime::parse(value.trim(), &Rfc3339)
        .map_err(|_| commercial_error(CommercialError::InvalidInput))
}

fn parse_plan_status(value: &str) -> Result<PlanStatus, HttpError> {
    PlanStatus::from_str(value).map_err(|_| commercial_error(CommercialError::InvalidInput))
}

fn parse_subscription_status(value: &str) -> Result<SubscriptionStatus, HttpError> {
    SubscriptionStatus::from_str(value).map_err(|_| commercial_error(CommercialError::InvalidInput))
}

fn idempotency_key(headers: &HeaderMap) -> Result<String, HttpError> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| commercial_error(CommercialError::InvalidInput))
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatusDto {
    Draft,
    Active,
    Maintenance,
    Deprecated,
    Retired,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatusDto {
    Active,
    Suspended,
    Cancelled,
    Expired,
}

impl From<PlanStatus> for PlanStatusDto {
    fn from(value: PlanStatus) -> Self {
        match value {
            PlanStatus::Draft => Self::Draft,
            PlanStatus::Active => Self::Active,
            PlanStatus::Maintenance => Self::Maintenance,
            PlanStatus::Deprecated => Self::Deprecated,
            PlanStatus::Retired => Self::Retired,
        }
    }
}

impl From<PlanStatusDto> for PlanStatus {
    fn from(value: PlanStatusDto) -> Self {
        match value {
            PlanStatusDto::Draft => Self::Draft,
            PlanStatusDto::Active => Self::Active,
            PlanStatusDto::Maintenance => Self::Maintenance,
            PlanStatusDto::Deprecated => Self::Deprecated,
            PlanStatusDto::Retired => Self::Retired,
        }
    }
}

impl From<SubscriptionStatus> for SubscriptionStatusDto {
    fn from(value: SubscriptionStatus) -> Self {
        match value {
            SubscriptionStatus::Active => Self::Active,
            SubscriptionStatus::Suspended => Self::Suspended,
            SubscriptionStatus::Cancelled => Self::Cancelled,
            SubscriptionStatus::Expired => Self::Expired,
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CommercialProductRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CommercialPlanRefResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
    pub is_trial: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PlanResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub product: CommercialProductRefResponse,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: PlanStatusDto,
    pub is_trial: bool,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
    pub subscription_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PlanListResponse {
    pub items: Vec<PlanResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SubscriptionResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: uuid::Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub product_id: uuid::Uuid,
    pub plan: CommercialPlanRefResponse,
    pub status: SubscriptionStatusDto,
    pub is_trial: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub ends_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub suspended_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub cancelled_at: Option<time::OffsetDateTime>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub renewed_from_subscription_id: Option<uuid::Uuid>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SubscriptionEventResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub event: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: time::OffsetDateTime,
    pub reason: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SubscriptionDetailResponse {
    pub subscription: SubscriptionResponse,
    pub events: Vec<SubscriptionEventResponse>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SubscriptionListResponse {
    pub items: Vec<SubscriptionResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct EntitlementsResponse {
    pub effective: bool,
    pub status: Option<SubscriptionStatusDto>,
    pub reason: String,
    pub plan: Option<CommercialPlanRefResponse>,
    pub features: BTreeMap<String, bool>,
    pub limits: BTreeMap<String, i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub starts_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ends_at: Option<time::OffsetDateTime>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlanListQuery {
    pub product_id: Option<String>,
    pub status: Option<String>,
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubscriptionListQuery {
    pub organization_id: Option<String>,
    pub product_id: Option<String>,
    pub status: Option<String>,
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct CreatePlanRequest {
    #[schema(value_type = String, format = Uuid)]
    pub product_id: uuid::Uuid,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: Option<PlanStatusDto>,
    #[serde(default)]
    pub is_trial: bool,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
    #[serde(default)]
    pub limits: BTreeMap<String, i64>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct UpdatePlanRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<PlanStatusDto>,
    pub features: Option<BTreeMap<String, bool>>,
    pub limits: Option<BTreeMap<String, i64>>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct CreateSubscriptionRequest {
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: uuid::Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub product_id: uuid::Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub plan_id: uuid::Uuid,
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct TransitionSubscriptionRequest {
    pub reason: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct RenewSubscriptionRequest {
    #[schema(value_type = Option<String>, format = Uuid)]
    pub plan_id: Option<uuid::Uuid>,
    pub ends_at: String,
}

fn plan_status_dto(value: PlanStatus) -> PlanStatusDto {
    value.into()
}

fn subscription_status_dto(value: SubscriptionStatus) -> SubscriptionStatusDto {
    value.into()
}

fn plan_response(plan: Plan) -> PlanResponse {
    PlanResponse {
        id: plan.id,
        product: CommercialProductRefResponse {
            id: plan.product_id.as_uuid(),
            key: plan.product_key,
            name: plan.product_name,
        },
        key: plan.key,
        name: plan.name,
        description: plan.description,
        status: plan_status_dto(plan.status),
        is_trial: plan.is_trial,
        features: plan.features,
        limits: plan.limits,
        subscription_count: plan.subscription_count,
        created_at: plan.created_at,
        updated_at: plan.updated_at,
    }
}

fn subscription_response(subscription: Subscription) -> SubscriptionResponse {
    SubscriptionResponse {
        id: subscription.id,
        organization_id: subscription.organization_id.as_uuid(),
        product_id: subscription.product_id.as_uuid(),
        plan: CommercialPlanRefResponse {
            id: subscription.plan.id,
            key: subscription.plan.key,
            name: subscription.plan.name,
            is_trial: subscription.plan.is_trial,
        },
        status: subscription_status_dto(subscription.status),
        is_trial: subscription.is_trial,
        starts_at: subscription.starts_at,
        ends_at: subscription.ends_at,
        created_at: subscription.created_at,
        updated_at: subscription.updated_at,
        suspended_at: subscription.suspended_at,
        cancelled_at: subscription.cancelled_at,
        renewed_from_subscription_id: subscription.renewed_from_subscription_id,
    }
}

fn subscription_detail_response(detail: SubscriptionDetail) -> SubscriptionDetailResponse {
    SubscriptionDetailResponse {
        subscription: subscription_response(detail.subscription),
        events: detail.events.into_iter().map(event_response).collect(),
    }
}

fn event_response(event: SubscriptionEvent) -> SubscriptionEventResponse {
    SubscriptionEventResponse {
        id: event.id,
        event: event.event,
        from_status: event.from_status,
        to_status: event.to_status,
        occurred_at: event.occurred_at,
        reason: event.reason,
    }
}

fn entitlements_response(value: Entitlements) -> EntitlementsResponse {
    EntitlementsResponse {
        effective: value.effective,
        status: value.status.map(subscription_status_dto),
        reason: value.reason,
        plan: value.plan.map(|plan| CommercialPlanRefResponse {
            id: plan.id,
            key: plan.key,
            name: plan.name,
            is_trial: plan.is_trial,
        }),
        features: value.features,
        limits: value.limits,
        starts_at: value.starts_at,
        ends_at: value.ends_at,
    }
}

#[utoipa::path(get, path = "/api/v1/control/plans", summary = "Lista planes comerciales", operation_id = "listControlPlans", tag = "Control Center",
    security(("staffSession" = [])),
    params(PlanListQuery),
    responses(
        (status = 200, description = "Planes", body = PlanListResponse),
        (status = 400, description = "Consulta inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_plans(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<PlanListQuery>, QueryRejection>,
) -> Result<Json<PlanListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = PlanFilters {
        product_id: query
            .product_id
            .as_deref()
            .map(parse_uuid)
            .transpose()?
            .map(ProductId::from_uuid),
        status: query.status.as_deref().map(parse_plan_status).transpose()?,
    };
    let page = auth_state
        .staff
        .commercial
        .list_plans(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(commercial_error)?;
    Ok(Json(PlanListResponse {
        items: page.items.into_iter().map(plan_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/plans/{plan_id}", summary = "Obtiene un plan comercial", operation_id = "getControlPlan", tag = "Control Center",
    security(("staffSession" = [])),
    params(("plan_id" = String, Path, description = "UUID del plan")),
    responses(
        (status = 200, description = "Plan", body = PlanResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Plan no encontrado", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_plan(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<PlanResponse>, HttpError> {
    let Path(plan_id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let plan = auth_state
        .staff
        .commercial
        .get_plan(&actor, parse_uuid(&plan_id)?)
        .await
        .map_err(commercial_error)?;
    Ok(Json(plan_response(plan)))
}

#[utoipa::path(post, path = "/api/v1/control/plans", summary = "Crea un plan comercial", operation_id = "createControlPlan", tag = "Control Center",
    security(("staffSession" = [])),
    request_body = CreatePlanRequest,
    responses(
        (status = 200, description = "Plan creado", body = PlanResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Producto no encontrado", body = ErrorResponse),
        (status = 409, description = "Plan duplicado o trial ya existente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn create_control_plan(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    body: Result<Json<CreatePlanRequest>, JsonRejection>,
) -> Result<Json<PlanResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Json(body) = body.map_err(body_error)?;
    let actor = actor(&auth_state, &context).await?;
    let plan = auth_state
        .staff
        .commercial
        .create_plan(
            &actor,
            NewPlan {
                product_id: ProductId::from_uuid(body.product_id),
                key: body.key,
                name: body.name,
                description: body.description,
                status: body
                    .status
                    .map(PlanStatus::from)
                    .unwrap_or(PlanStatus::Draft),
                is_trial: body.is_trial,
                features: body.features,
                limits: body.limits,
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(plan_response(plan)))
}

#[utoipa::path(patch, path = "/api/v1/control/plans/{plan_id}", summary = "Actualiza un plan comercial", operation_id = "updateControlPlan", tag = "Control Center",
    security(("staffSession" = [])),
    params(("plan_id" = String, Path, description = "UUID del plan")),
    request_body = UpdatePlanRequest,
    responses(
        (status = 200, description = "Plan actualizado", body = PlanResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Plan no encontrado", body = ErrorResponse),
        (status = 409, description = "Condiciones inmutables o transición inválida", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn update_control_plan(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<UpdatePlanRequest>, JsonRejection>,
) -> Result<Json<PlanResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(plan_id) = path.map_err(path_error)?;
    let Json(body) = body.map_err(body_error)?;
    let actor = actor(&auth_state, &context).await?;
    let plan = auth_state
        .staff
        .commercial
        .update_plan(
            &actor,
            parse_uuid(&plan_id)?,
            PlanPatch {
                name: body.name,
                description: body.description,
                status: body.status.map(PlanStatus::from),
                features: body.features,
                limits: body.limits,
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(plan_response(plan)))
}

#[utoipa::path(get, path = "/api/v1/control/subscriptions", summary = "Lista suscripciones", operation_id = "listControlSubscriptions", tag = "Control Center",
    security(("staffSession" = [])),
    params(SubscriptionListQuery),
    responses(
        (status = 200, description = "Suscripciones", body = SubscriptionListResponse),
        (status = 400, description = "Consulta inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_control_subscriptions(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    query: Result<Query<SubscriptionListQuery>, QueryRejection>,
) -> Result<Json<SubscriptionListResponse>, HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let actor = actor(&auth_state, &context).await?;
    let filters = SubscriptionFilters {
        organization_id: query
            .organization_id
            .as_deref()
            .map(parse_uuid)
            .transpose()?
            .map(OrganizationId::from_uuid),
        product_id: query
            .product_id
            .as_deref()
            .map(parse_uuid)
            .transpose()?
            .map(ProductId::from_uuid),
        status: query
            .status
            .as_deref()
            .map(parse_subscription_status)
            .transpose()?,
    };
    let page = auth_state
        .staff
        .commercial
        .list_subscriptions(&actor, filters, query.limit, query.cursor)
        .await
        .map_err(commercial_error)?;
    Ok(Json(SubscriptionListResponse {
        items: page.items.into_iter().map(subscription_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/v1/control/subscriptions/{subscription_id}", summary = "Obtiene una suscripción con historial", operation_id = "getControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    params(("subscription_id" = String, Path, description = "UUID de la suscripción")),
    responses(
        (status = 200, description = "Suscripción", body = SubscriptionDetailResponse),
        (status = 400, description = "Identificador inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Suscripción no encontrada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<SubscriptionDetailResponse>, HttpError> {
    let Path(subscription_id) = path.map_err(path_error)?;
    let actor = actor(&auth_state, &context).await?;
    let detail = auth_state
        .staff
        .commercial
        .get_subscription(&actor, parse_uuid(&subscription_id)?)
        .await
        .map_err(commercial_error)?;
    Ok(Json(subscription_detail_response(detail)))
}

#[utoipa::path(post, path = "/api/v1/control/subscriptions", summary = "Crea una suscripción", operation_id = "createControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    request_body = CreateSubscriptionRequest,
    responses(
        (status = 200, description = "Suscripción creada", body = SubscriptionResponse),
        (status = 400, description = "Solicitud inválida o Idempotency-Key ausente", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Organización o plan no encontrado", body = ErrorResponse),
        (status = 409, description = "Conflicto de estado o idempotencia", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn create_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    body: Result<Json<CreateSubscriptionRequest>, JsonRejection>,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let key = idempotency_key(&headers)?;
    let Json(body) = body.map_err(body_error)?;
    let actor = actor(&auth_state, &context).await?;
    let subscription = auth_state
        .staff
        .commercial
        .create_subscription(
            &actor,
            CreateSubscription {
                organization_id: OrganizationId::from_uuid(body.organization_id),
                product_id: ProductId::from_uuid(body.product_id),
                plan_id: body.plan_id,
                starts_at: parse_time(&body.starts_at)?,
                ends_at: parse_time(&body.ends_at)?,
                idempotency_key: key,
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(subscription_response(subscription)))
}

#[utoipa::path(post, path = "/api/v1/control/subscriptions/{subscription_id}/suspend", summary = "Suspende una suscripción", operation_id = "suspendControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    params(("subscription_id" = String, Path, description = "UUID de la suscripción")),
    request_body = TransitionSubscriptionRequest,
    responses(
        (status = 200, description = "Suscripción suspendida", body = SubscriptionResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Suscripción no encontrada", body = ErrorResponse),
        (status = 409, description = "Transición inválida", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn suspend_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<TransitionSubscriptionRequest>, JsonRejection>,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    transition(
        auth_state,
        context,
        headers,
        path,
        body,
        SubscriptionAction::Suspend,
    )
    .await
}

#[utoipa::path(post, path = "/api/v1/control/subscriptions/{subscription_id}/reactivate", summary = "Reactiva una suscripción", operation_id = "reactivateControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    params(("subscription_id" = String, Path, description = "UUID de la suscripción")),
    request_body = TransitionSubscriptionRequest,
    responses(
        (status = 200, description = "Suscripción reactivada", body = SubscriptionResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Suscripción no encontrada", body = ErrorResponse),
        (status = 409, description = "Transición inválida", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn reactivate_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<TransitionSubscriptionRequest>, JsonRejection>,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    transition(
        auth_state,
        context,
        headers,
        path,
        body,
        SubscriptionAction::Reactivate,
    )
    .await
}

#[utoipa::path(post, path = "/api/v1/control/subscriptions/{subscription_id}/cancel", summary = "Cancela una suscripción", operation_id = "cancelControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    params(("subscription_id" = String, Path, description = "UUID de la suscripción")),
    request_body = TransitionSubscriptionRequest,
    responses(
        (status = 200, description = "Suscripción cancelada", body = SubscriptionResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Suscripción no encontrada", body = ErrorResponse),
        (status = 409, description = "Transición inválida", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn cancel_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<TransitionSubscriptionRequest>, JsonRejection>,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    transition(
        auth_state,
        context,
        headers,
        path,
        body,
        SubscriptionAction::Cancel,
    )
    .await
}

async fn transition(
    auth_state: AuthState,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<TransitionSubscriptionRequest>, JsonRejection>,
    action: SubscriptionAction,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let Path(subscription_id) = path.map_err(path_error)?;
    let Json(body) = body.map_err(body_error)?;
    let actor = actor(&auth_state, &context).await?;
    let subscription = auth_state
        .staff
        .commercial
        .transition_subscription(
            &actor,
            parse_uuid(&subscription_id)?,
            action,
            SubscriptionTransition {
                reason: body.reason,
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(subscription_response(subscription)))
}

#[utoipa::path(post, path = "/api/v1/control/subscriptions/{subscription_id}/renew", summary = "Renueva una suscripción", operation_id = "renewControlSubscription", tag = "Control Center",
    security(("staffSession" = [])),
    params(("subscription_id" = String, Path, description = "UUID de la suscripción")),
    request_body = RenewSubscriptionRequest,
    responses(
        (status = 200, description = "Nueva suscripción", body = SubscriptionResponse),
        (status = 400, description = "Solicitud inválida o Idempotency-Key ausente", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Suscripción o plan no encontrado", body = ErrorResponse),
        (status = 409, description = "Transición o idempotencia inválida", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn renew_control_subscription(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<RenewSubscriptionRequest>, JsonRejection>,
) -> Result<Json<SubscriptionResponse>, HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    let key = idempotency_key(&headers)?;
    let Path(subscription_id) = path.map_err(path_error)?;
    let Json(body) = body.map_err(body_error)?;
    let actor = actor(&auth_state, &context).await?;
    let subscription = auth_state
        .staff
        .commercial
        .renew_subscription(
            &actor,
            parse_uuid(&subscription_id)?,
            RenewSubscription {
                plan_id: body.plan_id,
                ends_at: parse_time(&body.ends_at)?,
                idempotency_key: key,
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(subscription_response(subscription)))
}

#[utoipa::path(get, path = "/api/v1/control/organizations/{organization_id}/entitlements", summary = "Resuelve entitlements efectivos", operation_id = "getControlEntitlements", tag = "Control Center",
    security(("staffSession" = [])),
    params(
        ("organization_id" = String, Path, description = "UUID de la organización"),
        ("product_id" = String, Query, description = "UUID del producto")
    ),
    responses(
        (status = 200, description = "Entitlements efectivos", body = EntitlementsResponse),
        (status = 400, description = "Solicitud inválida", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 404, description = "Organización no encontrada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn get_control_entitlements(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<EntitlementsQuery>, QueryRejection>,
) -> Result<Json<EntitlementsResponse>, HttpError> {
    let Path(organization_id) = path.map_err(path_error)?;
    let Query(query) = query.map_err(query_error)?;
    let product_id = query
        .product_id
        .as_deref()
        .ok_or_else(|| commercial_error(CommercialError::InvalidInput))?;
    let actor = actor(&auth_state, &context).await?;
    let value = auth_state
        .staff
        .commercial
        .entitlements(
            &actor,
            OrganizationId::from_uuid(parse_uuid(&organization_id)?),
            ProductId::from_uuid(parse_uuid(product_id)?),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(entitlements_response(value)))
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EntitlementsQuery {
    pub product_id: Option<String>,
}
