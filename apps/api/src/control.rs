//! Endpoints internos del registro de productos y proyectos.
//!
//! Toda operación exige `staffSession`, se autoriza por capability en la capa de
//! aplicación y audita las mutaciones. No reutiliza endpoints comerciales.

use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
};
use platform_application::product::StaffActor;
use platform_core::product::{
    LifecycleStatus, NewProduct, NewProject, Product, ProductChanges, ProductError, ProductType,
    Project, ProjectChanges, ProjectType, is_valid_key,
};

use crate::auth::{self, AuthState};
use crate::contract::ErrorResponse;
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

fn product_error(error: ProductError) -> HttpError {
    match error {
        ProductError::NotFound => {
            auth::error(StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        ProductError::Forbidden => auth::error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "capability insuficiente para la operación",
        ),
        ProductError::InvalidInput => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        ProductError::Conflict => {
            auth::error(StatusCode::CONFLICT, "conflict", "la clave ya existe")
        }
        ProductError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
}

async fn actor(auth_state: &AuthState, context: &StaffContext) -> Result<StaffActor, HttpError> {
    let identity = auth_state
        .staff
        .service
        .identity(context.session.account_id)
        .await
        .map_err(crate::staff::staff_error)?;
    Ok(StaffActor {
        account_id: identity.account_id,
        role: identity.role,
    })
}

pub(crate) fn require_origin(headers: &HeaderMap, auth_state: &AuthState) -> Result<(), HttpError> {
    if auth::origin_allowed(headers, auth_state) {
        Ok(())
    } else {
        Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ))
    }
}

// Enums de contrato: exponen los valores permitidos al cliente generado.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProductStatusDto {
    Development,
    Active,
    Maintenance,
    Deprecated,
    Retired,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProductTypeDto {
    Desktop,
    Saas,
    Web,
    Mobile,
    Service,
    Integration,
    Internal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTypeDto {
    Service,
    Application,
    Integration,
    Infrastructure,
    Initiative,
}

impl From<LifecycleStatus> for ProductStatusDto {
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

impl From<ProductStatusDto> for LifecycleStatus {
    fn from(value: ProductStatusDto) -> Self {
        match value {
            ProductStatusDto::Development => Self::Development,
            ProductStatusDto::Active => Self::Active,
            ProductStatusDto::Maintenance => Self::Maintenance,
            ProductStatusDto::Deprecated => Self::Deprecated,
            ProductStatusDto::Retired => Self::Retired,
        }
    }
}

impl From<ProductType> for ProductTypeDto {
    fn from(value: ProductType) -> Self {
        match value {
            ProductType::Desktop => Self::Desktop,
            ProductType::Saas => Self::Saas,
            ProductType::Web => Self::Web,
            ProductType::Mobile => Self::Mobile,
            ProductType::Service => Self::Service,
            ProductType::Integration => Self::Integration,
            ProductType::Internal => Self::Internal,
        }
    }
}

impl From<ProductTypeDto> for ProductType {
    fn from(value: ProductTypeDto) -> Self {
        match value {
            ProductTypeDto::Desktop => Self::Desktop,
            ProductTypeDto::Saas => Self::Saas,
            ProductTypeDto::Web => Self::Web,
            ProductTypeDto::Mobile => Self::Mobile,
            ProductTypeDto::Service => Self::Service,
            ProductTypeDto::Integration => Self::Integration,
            ProductTypeDto::Internal => Self::Internal,
        }
    }
}

impl From<ProjectType> for ProjectTypeDto {
    fn from(value: ProjectType) -> Self {
        match value {
            ProjectType::Service => Self::Service,
            ProjectType::Application => Self::Application,
            ProjectType::Integration => Self::Integration,
            ProjectType::Infrastructure => Self::Infrastructure,
            ProjectType::Initiative => Self::Initiative,
        }
    }
}

impl From<ProjectTypeDto> for ProjectType {
    fn from(value: ProjectTypeDto) -> Self {
        match value {
            ProjectTypeDto::Service => Self::Service,
            ProjectTypeDto::Application => Self::Application,
            ProjectTypeDto::Integration => Self::Integration,
            ProjectTypeDto::Infrastructure => Self::Infrastructure,
            ProjectTypeDto::Initiative => Self::Initiative,
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProductResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: ProductStatusDto,
    pub product_type: ProductTypeDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProductListResponse {
    pub items: Vec<ProductResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProjectResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub project_type: ProjectTypeDto,
    pub status: ProductStatusDto,
    pub product_id: Option<uuid::Uuid>,
    pub product_key: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProjectListResponse {
    pub items: Vec<ProjectResponse>,
    pub next_cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProductRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: ProductStatusDto,
    #[serde(rename = "type")]
    pub product_type: ProductTypeDto,
}

/// `key` y `type` son inmutables. Los campos omitidos o `null` no cambian. Los
/// campos presentes deben diferir del valor persistido; de lo contrario se rechaza.
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductRequest {
    #[schema(nullable = false)]
    pub name: Option<String>,
    #[schema(nullable = false)]
    pub description: Option<String>,
    #[schema(nullable = false)]
    pub status: Option<ProductStatusDto>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub project_type: ProjectTypeDto,
    pub status: ProductStatusDto,
    pub product_key: Option<String>,
}

/// `key`, `project_type` y la asociación Product son inmutables. Los campos
/// omitidos o `null` no cambian. Los campos presentes deben diferir del valor
/// persistido; de lo contrario se rechaza.
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectRequest {
    #[schema(nullable = false)]
    pub name: Option<String>,
    #[schema(nullable = false)]
    pub description: Option<String>,
    #[schema(nullable = false)]
    pub status: Option<ProductStatusDto>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProductListQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub product_type: Option<String>,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectListQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub product_key: Option<String>,
}

fn parse_status(value: &str) -> Result<LifecycleStatus, HttpError> {
    value
        .parse()
        .map_err(|_| product_error(ProductError::InvalidInput))
}

fn parse_product_type(value: &str) -> Result<ProductType, HttpError> {
    value
        .parse()
        .map_err(|_| product_error(ProductError::InvalidInput))
}

fn product_response(product: Product) -> ProductResponse {
    ProductResponse {
        id: product.id.as_uuid(),
        key: product.key,
        name: product.name,
        description: product.description,
        status: ProductStatusDto::from(product.status),
        product_type: ProductTypeDto::from(product.product_type),
        created_at: product.created_at,
        updated_at: product.updated_at,
    }
}

fn project_response(project: Project) -> ProjectResponse {
    ProjectResponse {
        id: project.id.as_uuid(),
        key: project.key,
        name: project.name,
        description: project.description,
        project_type: ProjectTypeDto::from(project.project_type),
        status: ProductStatusDto::from(project.status),
        product_id: project.product_id.map(|id| id.as_uuid()),
        product_key: project.product_key,
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}

#[utoipa::path(get, path = "/api/v1/control/products", summary = "Lista productos con paginación por clave", operation_id = "listControlProducts", tag = "Control Center",
    security(("staffSession" = [])),
    params(ProductListQuery),
    responses(
        (status = 200, description = "Productos paginados por clave", body = ProductListResponse),
        (status = 400, description = "Filtro o cursor inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn list_control_products(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    Query(query): Query<ProductListQuery>,
) -> Result<Json<ProductListResponse>, HttpError> {
    let actor = actor(&auth_state, &context).await?;
    let status = query.status.as_deref().map(parse_status).transpose()?;
    let product_type = query
        .product_type
        .as_deref()
        .map(parse_product_type)
        .transpose()?;
    let page = auth_state
        .staff
        .product
        .list_products(&actor, status, product_type, query.limit, query.cursor)
        .await
        .map_err(product_error)?;
    Ok(Json(ProductListResponse {
        items: page.items.into_iter().map(product_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(post, path = "/api/v1/control/products", summary = "Crea un producto con clave estable", operation_id = "createControlProduct", tag = "Control Center",
    security(("staffSession" = [])),
    request_body(content = CreateProductRequest),
    responses(
        (status = 201, description = "Producto creado", body = ProductResponse),
        (status = 400, description = "Clave, tipo o estado inválidos", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 409, description = "La clave ya existe", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn create_control_product(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    Json(request): Json<CreateProductRequest>,
) -> Result<(StatusCode, Json<ProductResponse>), HttpError> {
    require_origin(&headers, &auth_state)?;
    let actor = actor(&auth_state, &context).await?;
    let product = NewProduct {
        key: request.key,
        name: request.name,
        description: request.description,
        status: LifecycleStatus::from(request.status),
        product_type: ProductType::from(request.product_type),
    };
    let created = auth_state
        .staff
        .product
        .create_product(&actor, product, time::OffsetDateTime::now_utc())
        .await
        .map_err(product_error)?;
    Ok((StatusCode::CREATED, Json(product_response(created))))
}

#[utoipa::path(get, path = "/api/v1/control/products/{key}", summary = "Obtiene un producto por su clave", operation_id = "getControlProduct", tag = "Control Center",
    security(("staffSession" = [])),
    params(("key" = String, Path, description = "Clave estable del producto")),
    responses(
        (status = 200, description = "Producto", body = ProductResponse),
        (status = 400, description = "Clave inválida", body = ErrorResponse),
        (status = 404, description = "Producto no encontrado", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn get_control_product(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    Path(key): Path<String>,
) -> Result<Json<ProductResponse>, HttpError> {
    if !is_valid_key(&key) {
        return Err(product_error(ProductError::InvalidInput));
    }
    let actor = actor(&auth_state, &context).await?;
    let product = auth_state
        .staff
        .product
        .get_product(&actor, &key)
        .await
        .map_err(product_error)?;
    Ok(Json(product_response(product)))
}

#[utoipa::path(patch, path = "/api/v1/control/products/{key}", summary = "Actualiza un producto", operation_id = "updateControlProduct", tag = "Control Center",
    security(("staffSession" = [])),
    params(("key" = String, Path, description = "Clave estable del producto")),
    request_body(content = UpdateProductRequest),
    responses(
        (status = 200, description = "Producto actualizado", body = ProductResponse),
        (status = 400, description = "Cambios inválidos o sin cambios efectivos", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Producto no encontrado", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn update_control_product(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(request): Json<UpdateProductRequest>,
) -> Result<Json<ProductResponse>, HttpError> {
    require_origin(&headers, &auth_state)?;
    if !is_valid_key(&key) {
        return Err(product_error(ProductError::InvalidInput));
    }
    let actor = actor(&auth_state, &context).await?;
    let changes = ProductChanges {
        name: request.name,
        description: request.description,
        status: request.status.map(LifecycleStatus::from),
    };
    let updated = auth_state
        .staff
        .product
        .update_product(&actor, &key, changes, time::OffsetDateTime::now_utc())
        .await
        .map_err(product_error)?;
    Ok(Json(product_response(updated)))
}

#[utoipa::path(get, path = "/api/v1/control/projects", summary = "Lista proyectos con filtro por producto", operation_id = "listControlProjects", tag = "Control Center",
    security(("staffSession" = [])),
    params(ProjectListQuery),
    responses(
        (status = 200, description = "Proyectos paginados por clave", body = ProjectListResponse),
        (status = 400, description = "Filtro o cursor inválido", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn list_control_projects(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<ProjectListResponse>, HttpError> {
    let actor = actor(&auth_state, &context).await?;
    let page = auth_state
        .staff
        .product
        .list_projects(&actor, query.product_key, query.limit, query.cursor)
        .await
        .map_err(product_error)?;
    Ok(Json(ProjectListResponse {
        items: page.items.into_iter().map(project_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(post, path = "/api/v1/control/projects", summary = "Crea un proyecto", operation_id = "createControlProject", tag = "Control Center",
    security(("staffSession" = [])),
    request_body(content = CreateProjectRequest),
    responses(
        (status = 201, description = "Proyecto creado", body = ProjectResponse),
        (status = 400, description = "Clave, tipo, estado o producto inválidos", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 409, description = "La clave ya existe", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn create_control_project(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    Json(request): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectResponse>), HttpError> {
    require_origin(&headers, &auth_state)?;
    let actor = actor(&auth_state, &context).await?;
    let project = NewProject {
        key: request.key,
        name: request.name,
        description: request.description,
        project_type: ProjectType::from(request.project_type),
        status: LifecycleStatus::from(request.status),
        product_key: request.product_key,
    };
    let created = auth_state
        .staff
        .product
        .create_project(&actor, project, time::OffsetDateTime::now_utc())
        .await
        .map_err(product_error)?;
    Ok((StatusCode::CREATED, Json(project_response(created))))
}

#[utoipa::path(get, path = "/api/v1/control/projects/{key}", summary = "Obtiene un proyecto por su clave", operation_id = "getControlProject", tag = "Control Center",
    security(("staffSession" = [])),
    params(("key" = String, Path, description = "Clave estable del proyecto")),
    responses(
        (status = 200, description = "Proyecto", body = ProjectResponse),
        (status = 400, description = "Clave inválida", body = ErrorResponse),
        (status = 404, description = "Proyecto no encontrado", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn get_control_project(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    Path(key): Path<String>,
) -> Result<Json<ProjectResponse>, HttpError> {
    if !is_valid_key(&key) {
        return Err(product_error(ProductError::InvalidInput));
    }
    let actor = actor(&auth_state, &context).await?;
    let project = auth_state
        .staff
        .product
        .get_project(&actor, &key)
        .await
        .map_err(product_error)?;
    Ok(Json(project_response(project)))
}

#[utoipa::path(patch, path = "/api/v1/control/projects/{key}", summary = "Actualiza un proyecto", operation_id = "updateControlProject", tag = "Control Center",
    security(("staffSession" = [])),
    params(("key" = String, Path, description = "Clave estable del proyecto")),
    request_body(content = UpdateProjectRequest),
    responses(
        (status = 200, description = "Proyecto actualizado", body = ProjectResponse),
        (status = 400, description = "Cambios inválidos o sin cambios efectivos", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Proyecto no encontrado", body = ErrorResponse),
        (status = 503, description = "Almacenamiento no disponible", body = ErrorResponse)))]
pub(crate) async fn update_control_project(
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, HttpError> {
    require_origin(&headers, &auth_state)?;
    if !is_valid_key(&key) {
        return Err(product_error(ProductError::InvalidInput));
    }
    let actor = actor(&auth_state, &context).await?;
    let changes = ProjectChanges {
        name: request.name,
        description: request.description,
        status: request.status.map(LifecycleStatus::from),
    };
    let updated = auth_state
        .staff
        .product
        .update_project(&actor, &key, changes, time::OffsetDateTime::now_utc())
        .await
        .map_err(product_error)?;
    Ok(Json(project_response(updated)))
}
