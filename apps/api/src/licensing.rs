//! Rutas legacy/DEV de licenciamiento. No forman parte del flujo productivo.

use crate::*;

/// Devuelve el catálogo técnico. Los nombres de plan no implican disponibilidad comercial.
#[utoipa::path(get, path = "/api/v1/plans", operation_id = "listPlans", tag = "Planes",
    responses(
        (status = 200, description = "Catálogo de planes", body = [PlanResponse],
            example = json!([{"plan":"trial","display_name":"Trial (30 días)","features":["catalog","sales"]}]))))]
pub(crate) async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlanResponse>>, HttpError> {
    state
        .service
        .list_plans()
        .await
        .map(|plans| Json(plans.into_iter().map(Into::into).collect()))
        .map_err(err)
}

/// Registra una instalación técnica; requiere el token administrativo DEV.
#[utoipa::path(post, path = "/api/v1/installations", operation_id = "registerInstallation", tag = "Instalaciones",
    request_body(content = RegisterInstallationBody, description = "organization_id UUID y device_label no vacío; installation_id UUID opcional"),
    security(("devAdminToken" = [])),
    responses(
        (status = 201, description = "Instalación registrada", body = InstallationResponse),
        (status = 400, description = "Etiqueta vacía o JSON malformado", body = ErrorResponse),
        (status = 401, description = "Token ausente, incorrecto o no configurado", body = ErrorResponse),
        (status = 404, description = "Organización comercial inexistente", body = ErrorResponse),
        (status = 409, description = "Instalación o etiqueta ya registrada", body = ErrorResponse),
        (status = 415, description = "Content-Type distinto de application/json", body = ErrorResponse),
        (status = 422, description = "JSON incompatible con el esquema", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn register_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<RegisterInstallationBody>, JsonRejection>,
) -> Result<(StatusCode, Json<InstallationResponse>), HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let Json(request) = request.map_err(json_error)?;
    let view = state
        .service
        .register_installation(request.into(), time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view.into())))
}

/// Consulta una instalación; el identificador no concede autorización por sí solo.
#[utoipa::path(get, path = "/api/v1/installations/{id}", operation_id = "getInstallation", tag = "Instalaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Instalación encontrada", body = InstallationResponse),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn get_installation(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<InstallationResponse>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .get_installation(path_id(id)?)
        .await
        .map_err(err)?;
    Ok(Json(view.into()))
}

/// Activa una sola vez el trial técnico de 30 días para una instalación.
#[utoipa::path(post, path = "/api/v1/installations/{id}/trial", operation_id = "startTrial", tag = "Instalaciones",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Trial activado", body = InstallationResponse),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 409, description = "Trial activado anteriormente", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn start_trial(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<InstallationResponse>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let view = state
        .service
        .start_trial(path_id(id)?, time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok(Json(view.into()))
}

/// Emite una autorización de desarrollo sólo cuando el firmante DEV está habilitado.
#[utoipa::path(post, path = "/api/v1/installations/{id}/licenses", operation_id = "issueLicense", tag = "Licencias DEV",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    request_body(content = IssueLicenseBody, description = "plan: trial, local, pro o enterprise; valid_until debe ser futuro"),
    security(("devAdminToken" = [])),
    responses(
        (status = 201, description = "Autorización DEV firmada", body = IssuedLicenseResponse),
        (status = 400, description = "UUID, plan, vigencia o JSON malformado", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 404, description = "Instalación inexistente", body = ErrorResponse),
        (status = 415, description = "Content-Type distinto de application/json", body = ErrorResponse),
        (status = 422, description = "JSON incompatible con el esquema", body = ErrorResponse),
        (status = 503, description = "Emisión DEV o PostgreSQL indisponibles", body = ErrorResponse)))]
pub(crate) async fn issue_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<IssueLicenseBody>, JsonRejection>,
) -> Result<(StatusCode, Json<IssuedLicenseResponse>), HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    if !state.licensing_enabled {
        return Err(err(AppError::LicensingDisabled));
    }
    let id = path_id(id)?;
    let Json(request) = request.map_err(json_error)?;
    let view = state
        .service
        .issue_license(id, request.into(), time::OffsetDateTime::now_utc())
        .await
        .map_err(err)?;
    Ok((StatusCode::CREATED, Json(view.into())))
}

/// Lista las autorizaciones registradas; para UUID desconocidos devuelve lista vacía.
#[utoipa::path(get, path = "/api/v1/installations/{id}/licenses", operation_id = "listLicenses", tag = "Licencias DEV",
    params(("id" = Uuid, Path, description = "Identificador UUID de la instalación")),
    security(("devAdminToken" = [])),
    responses(
        (status = 200, description = "Autorizaciones; lista vacía si no existen", body = [LicenseRecordResponse], example = json!([])),
        (status = 400, description = "UUID de ruta inválido", body = ErrorResponse),
        (status = 401, description = "Token ausente o incorrecto", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn list_licenses(
    State(state): State<AppState>,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<Vec<LicenseRecordResponse>>, HttpError> {
    admin_guard(&headers, &state.admin_token).map_err(err)?;
    let licenses = state
        .service
        .list_licenses(path_id(id)?)
        .await
        .map_err(err)?;
    Ok(Json(licenses.into_iter().map(Into::into).collect()))
}
