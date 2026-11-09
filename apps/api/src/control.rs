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

fn require_origin(headers: &HeaderMap, auth_state: &AuthState) -> Result<(), HttpError> {
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

fn parse_project_type(value: &str) -> Result<ProjectType, HttpError> {
    value
        .parse()
        .map_err(|_| product_error(ProductError::InvalidInput))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProductResponse {
    #[schema(value_type = String, format = Uuid)]
    pub id: uuid::Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub product_type: String,
    pub created_at: time::OffsetDateTime,
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
    pub project_type: String,
    pub status: String,
    pub product_id: Option<uuid::Uuid>,
    pub product_key: Option<String>,
    pub created_at: time::OffsetDateTime,
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
    pub status: String,
    #[serde(rename = "type")]
    pub product_type: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub project_type: String,
    pub status: String,
    pub product_key: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
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

fn product_response(product: Product) -> ProductResponse {
    ProductResponse {
        id: product.id.as_uuid(),
        key: product.key,
        name: product.name,
        description: product.description,
        status: product.status.as_str().to_owned(),
        product_type: product.product_type.as_str().to_owned(),
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
        project_type: project.project_type.as_str().to_owned(),
        status: project.status.as_str().to_owned(),
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
        status: parse_status(&request.status)?,
        product_type: parse_product_type(&request.product_type)?,
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
        (status = 400, description = "Cambios inválidos", body = ErrorResponse),
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
        status: request.status.as_deref().map(parse_status).transpose()?,
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
        project_type: parse_project_type(&request.project_type)?,
        status: parse_status(&request.status)?,
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
        (status = 400, description = "Cambios inválidos", body = ErrorResponse),
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
        status: request.status.as_deref().map(parse_status).transpose()?,
    };
    let updated = auth_state
        .staff
        .product
        .update_project(&actor, &key, changes, time::OffsetDateTime::now_utc())
        .await
        .map_err(product_error)?;
    Ok(Json(project_response(updated)))
}
