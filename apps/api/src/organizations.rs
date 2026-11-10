//! Organizaciones comerciales y membresías desde la sesión comercial.

use crate::*;

pub(crate) fn commercial_error(error: CommercialError) -> HttpError {
    match error {
        CommercialError::NotFound => {
            auth::error(StatusCode::NOT_FOUND, "not_found", "recurso no encontrado")
        }
        CommercialError::Forbidden => auth::error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "rol insuficiente para la operación",
        ),
        CommercialError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "conflict",
            "el recurso ya existe o entra en conflicto",
        ),
        CommercialError::InvalidName
        | CommercialError::InvalidEmail
        | CommercialError::InvalidCursor => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud inválida",
        ),
        // Rol/versión inválidos en datos persistidos o fallo de almacenamiento.
        CommercialError::InvalidRole | CommercialError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct OrganizationResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174010")]
    pub id: Uuid,
    #[schema(example = "Mi negocio")]
    pub display_name: String,
    #[schema(example = "owner")]
    pub role: String,
    #[schema(example = "active")]
    pub status: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct OrganizationListResponse {
    pub items: Vec<OrganizationResponse>,
    /// Cursor opaco para la página siguiente; ausente en la última página.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(
        example = "eyJuIjoiTWkgbmVnb2NpbyIsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTAifQ"
    )]
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct MemberResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "member")]
    pub role: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime, example = "2026-11-08T00:00:00Z")]
    pub joined_at: time::OffsetDateTime,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct MemberListResponse {
    pub items: Vec<MemberResponse>,
    /// Cursor opaco para la página siguiente; ausente en la última página.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(
        example = "eyJ0IjoxNzYyNTYwMDAwMDAwMDAwMDAsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTEifQ"
    )]
    pub next_cursor: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct MemberRoleResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "admin")]
    pub role: String,
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct PageQuery {
    #[param(example = 50, minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
    #[param(
        example = "eyJuIjoiTWkgbmVnb2NpbyIsImkiOiIxMjNlNDU2Ny1lODliLTQyZDMtYTQ1Ni00MjY2MTQxNzQwMTAifQ"
    )]
    pub cursor: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddMemberRequest {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174011")]
    pub account_id: Uuid,
    #[schema(example = "member")]
    pub role: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeMemberRoleRequest {
    #[schema(example = "admin")]
    pub role: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateOrganizationRequest {
    #[schema(example = "Mi negocio", min_length = 1, max_length = 200)]
    pub display_name: String,
}

pub(crate) fn organization_response(view: OrganizationView) -> OrganizationResponse {
    OrganizationResponse {
        id: view.id.as_uuid(),
        display_name: view.display_name,
        role: view.role.as_str().to_owned(),
        status: view.status.to_owned(),
    }
}

pub(crate) fn member_response(membership: OrganizationMembership) -> MemberResponse {
    MemberResponse {
        account_id: membership.account_id.as_uuid(),
        role: membership.role.as_str().to_owned(),
        joined_at: membership.joined_at,
    }
}

pub(crate) fn page_params(
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<(Option<String>, u32), HttpError> {
    let Query(query) = query.map_err(query_error)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "limit debe estar entre 1 y 100",
        ));
    }
    let cursor = query.cursor.filter(|value| !value.is_empty());
    Ok((cursor, limit))
}

/// Convierte el rol del cliente; un valor fuera de owner/admin/member es 400.
pub(crate) fn role_from_request(role: &str) -> Result<OrganizationRole, HttpError> {
    OrganizationRole::from_storage(role, OrganizationRole::VERSION)
        .map_err(|_| auth::error(StatusCode::BAD_REQUEST, "validation_error", "rol inválido"))
}

/// Lista las organizaciones activas del actor autenticado con paginación por cursor.
#[utoipa::path(get, path = "/api/v1/organizations", operation_id = "listOrganizations", tag = "Organizaciones",
    params(PageQuery),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Organizaciones activas con membresía activa; orden por nombre e id. Usa next_cursor para continuar", body = OrganizationListResponse),
        (status = 400, description = "Cursor o límite inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn list_organizations(
    State(state): State<AppState>,
    auth_context: AuthContext,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<OrganizationListResponse>, HttpError> {
    let (cursor, limit) = page_params(query)?;
    let page = state
        .commercial
        .list_organizations(auth_context.session.account_id, cursor.as_deref(), limit)
        .await
        .map_err(commercial_error)?;
    Ok(Json(OrganizationListResponse {
        items: page.items.into_iter().map(organization_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

/// Crea una organización activa y asigna owner a la cuenta autenticada.
#[utoipa::path(post, path = "/api/v1/organizations", operation_id = "createOrganization", tag = "Organizaciones",
    request_body(content = CreateOrganizationRequest, description = "Nombre visible; la cuenta autenticada recibe owner"),
    security(("commercialSession" = [])),
    responses(
        (status = 201, description = "Organización creada; cabecera Location con la ruta del recurso", body = OrganizationResponse,
            headers(("Location" = String, description = "URL del recurso creado"))),
        (status = 400, description = "Nombre inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 409, description = "Conflicto de creación", body = ErrorResponse),
        (status = 429, description = "Demasiadas creaciones", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn create_organization(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let key = format!("organization-create:{}", auth_context.session.account_id);
    if state.organization_limiter.is_limited(&key) {
        return Err(auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiadas creaciones; inténtalo más tarde",
        ));
    }
    state.organization_limiter.record(&key);
    let created = state
        .commercial
        .create_organization(
            auth_context.session.account_id,
            &request.display_name,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    // La ventana de creación cuenta también los intentos exitosos.
    let location = format!("/api/v1/organizations/{}", created.id);
    let body = OrganizationResponse {
        id: created.id.as_uuid(),
        display_name: created.display_name,
        role: "owner".into(),
        status: "active".into(),
    };
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response.headers_mut().insert(
        axum::http::header::LOCATION,
        axum::http::HeaderValue::from_str(&location)
            .map_err(|_| commercial_error(CommercialError::Storage))?,
    );
    Ok(response)
}

/// Consulta una organización propia; un UUID ajeno responde 404 neutral.
#[utoipa::path(get, path = "/api/v1/organizations/{id}", operation_id = "getOrganization", tag = "Organizaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización")),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Organización activa con membresía activa", body = OrganizationResponse),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 404, description = "No existe o pertenece a otra cuenta", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn get_organization(
    State(state): State<AppState>,
    auth_context: AuthContext,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<OrganizationResponse>, HttpError> {
    let Path(id) = id.map_err(path_error)?;
    let organization = state
        .commercial
        .get_organization(
            auth_context.session.account_id,
            platform_core::ids::OrganizationId::from_uuid(id),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(organization_response(organization)))
}

/// Lista las membresías activas de una organización accesible para el actor.
#[utoipa::path(get, path = "/api/v1/organizations/{id}/members", operation_id = "listOrganizationMembers", tag = "Membresías",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización"), PageQuery),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Membresías activas visibles para owner, admin o member; orden por alta y cuenta", body = MemberListResponse),
        (status = 400, description = "UUID, cursor o límite inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 404, description = "Organización inexistente, ajena o sin membresía activa", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn list_members(
    State(state): State<AppState>,
    auth_context: AuthContext,
    id: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Json<MemberListResponse>, HttpError> {
    let Path(id) = id.map_err(path_error)?;
    let (cursor, limit) = page_params(query)?;
    let page = state
        .commercial
        .list_members(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            cursor.as_deref(),
            limit,
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(MemberListResponse {
        items: page.items.into_iter().map(member_response).collect(),
        next_cursor: page.next_cursor,
    }))
}

/// Agrega una cuenta activa como miembro; sólo un owner de la organización.
#[utoipa::path(post, path = "/api/v1/organizations/{id}/members", operation_id = "addOrganizationMember", tag = "Membresías",
    params(("id" = Uuid, Path, description = "Identificador UUID de la organización")),
    request_body(content = AddMemberRequest, description = "account_id de la cuenta activa y rol owner/admin/member"),
    security(("commercialSession" = [])),
    responses(
        (status = 201, description = "Membresía creada; cabecera Location con la ruta del recurso", body = MemberRoleResponse,
            headers(("Location" = String, description = "URL del recurso creado"))),
        (status = 400, description = "UUID o rol inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena, cuenta inexistente o deshabilitada", body = ErrorResponse),
        (status = 409, description = "La cuenta ya tuvo o tiene membresía en la organización", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn add_member(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<AddMemberRequest>, JsonRejection>,
) -> Result<impl IntoResponse, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path(id) = id.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let role = role_from_request(&request.role)?;
    state
        .commercial
        .add_member(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(request.account_id),
            role,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    let body = MemberRoleResponse {
        account_id: request.account_id,
        role: role.as_str().to_owned(),
    };
    let location = format!("/api/v1/organizations/{id}/members/{}", request.account_id);
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response.headers_mut().insert(
        axum::http::header::LOCATION,
        axum::http::HeaderValue::from_str(&location)
            .map_err(|_| commercial_error(CommercialError::Storage))?,
    );
    Ok(response)
}

/// Cambia el rol de un miembro; sólo un owner y sin romper el último propietario.
#[utoipa::path(patch, path = "/api/v1/organizations/{id}/members/{account_id}", operation_id = "changeOrganizationMemberRole", tag = "Membresías",
    params(
        ("id" = Uuid, Path, description = "Identificador UUID de la organización"),
        ("account_id" = Uuid, Path, description = "Identificador UUID de la cuenta miembro")),
    request_body(content = ChangeMemberRoleRequest, description = "Nuevo rol owner/admin/member"),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Rol actualizado", body = MemberRoleResponse),
        (status = 400, description = "UUID o rol inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena o membresía inexistente", body = ErrorResponse),
        (status = 409, description = "Rol sin cambios o último propietario protegido", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn change_member_role(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    request: Result<Json<ChangeMemberRoleRequest>, JsonRejection>,
) -> Result<Json<MemberRoleResponse>, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path((id, account_id)) = path.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let role = role_from_request(&request.role)?;
    state
        .commercial
        .change_role(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(account_id),
            role,
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(Json(MemberRoleResponse {
        account_id,
        role: role.as_str().to_owned(),
    }))
}

/// Revoca la membresía activa; sólo un owner y sin dejar la organización sin owner.
#[utoipa::path(delete, path = "/api/v1/organizations/{id}/members/{account_id}", operation_id = "revokeOrganizationMember", tag = "Membresías",
    params(
        ("id" = Uuid, Path, description = "Identificador UUID de la organización"),
        ("account_id" = Uuid, Path, description = "Identificador UUID de la cuenta miembro")),
    security(("commercialSession" = [])),
    responses(
        (status = 204, description = "Membresía revocada; deja de conceder acceso"),
        (status = 400, description = "UUID inválido", body = ErrorResponse),
        (status = 401, description = "Sesión inválida o cuenta deshabilitada", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o actor sin rol owner", body = ErrorResponse),
        (status = 404, description = "Organización ajena o membresía inexistente", body = ErrorResponse),
        (status = 409, description = "Último propietario protegido", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn revoke_member(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<StatusCode, HttpError> {
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path((id, account_id)) = path.map_err(path_error)?;
    state
        .commercial
        .revoke_member(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            CommercialAccountId::from_uuid(account_id),
            time::OffsetDateTime::now_utc(),
        )
        .await
        .map_err(commercial_error)?;
    Ok(StatusCode::NO_CONTENT)
}
