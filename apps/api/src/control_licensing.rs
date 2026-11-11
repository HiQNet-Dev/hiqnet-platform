//! Emisión controlada de licencias comerciales (staffSession, sólo Control).
//!
//! La solicitud sólo aporta la instalación y el producto: el plan, las
//! capacidades y la vigencia se resuelven en el dominio comercial
//! (`entitlements`) y se firman tal cual. Ninguna fecha ni característica
//! proveniente del cliente HTTP entra al documento.

use axum::{
    Extension, Json,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
};
use platform_application::commercial_domain::CommercialActor;
use platform_application::{AppError, license_issuance::EntitledPlan};
use platform_core::ids::{InstallationId, OrganizationId, ProductId};
use platform_core::staff::Capability;
use uuid::Uuid;

use crate::app_state::{AppState, err, json_error, path_error};
use crate::auth::AuthState;
use crate::contract::{ErrorResponse, IssuedLicenseResponse};
use crate::staff::StaffContext;

type HttpError = (StatusCode, Json<ErrorResponse>);

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct IssueControlLicenseRequest {
    #[schema(value_type = String, format = Uuid)]
    pub product_id: Uuid,
}

/// Traduce los errores de emisión a la superficie HTTP. La auditoría del
/// rechazo se resuelve en el handler, no aquí.
fn issuance_error(error: AppError) -> HttpError {
    let status = match error {
        AppError::NotFound => StatusCode::NOT_FOUND,
        AppError::Conflict => StatusCode::CONFLICT,
        // Sesión válida sin capacidad suficiente: 403. 401 queda reservado a la
        // ausencia de sesión, que resuelve StaffContext antes de llegar aquí.
        AppError::Unauthorized | AppError::NotEntitled => StatusCode::FORBIDDEN,
        AppError::UnsupportedPlan | AppError::UnsupportedFeature => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        AppError::LicensingDisabled | AppError::Storage => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    let code = if matches!(error, AppError::Unauthorized) {
        "forbidden"
    } else {
        error.code()
    };
    (
        status,
        Json(ErrorResponse {
            code,
            message: error.to_string(),
        }),
    )
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

/// Registra el rechazo cuando identidad, instalación y producto son conocidos.
/// Un fallo de auditoría no oculta la causa original.
async fn audit_denial(
    state: &AppState,
    installation_id: InstallationId,
    organization_id: OrganizationId,
    product_id: ProductId,
    actor: &CommercialActor,
    reason: &'static str,
) {
    let _ = state
        .service
        .record_issuance_denial(
            installation_id,
            organization_id,
            product_id.as_uuid(),
            actor,
            reason,
            time::OffsetDateTime::now_utc(),
        )
        .await;
}

/// Emite una licencia firmada a partir del derecho comercial vigente.
#[utoipa::path(post, path = "/api/v1/control/installations/{installation_id}/licenses", summary = "Emite una licencia firmada desde el derecho comercial vigente", operation_id = "issueControlLicense", tag = "Licencias",
    security(("staffSession" = [])),
    params(("installation_id" = String, Path, description = "UUID de la instalación")),
    request_body(content = IssueControlLicenseRequest, description = "product_id: UUID del producto contratado; plan, features y vigencia se resuelven en Platform"),
    responses(
        (status = 201, description = "Licencia firmada y auditada", body = IssuedLicenseResponse),
        (status = 400, description = "UUID o JSON malformado", body = ErrorResponse),
        (status = 401, description = "Sesión staff requerida", body = ErrorResponse),
        (status = 403, description = "Capability insuficiente u origen no permitido", body = ErrorResponse),
        (status = 404, description = "Instalación u organización inexistentes; una instalación ajena no se revela", body = ErrorResponse),
        (status = 409, description = "Ya existe una licencia para la misma suscripción", body = ErrorResponse),
        (status = 422, description = "Plan o capacidad del plan no representables en el contrato v1", body = ErrorResponse),
        (status = 415, description = "Content-Type distinto de application/json", body = ErrorResponse),
        (status = 503, description = "Emisión deshabilitada o PostgreSQL no disponible", body = ErrorResponse)))]
pub(crate) async fn issue_control_license(
    State(state): State<AppState>,
    Extension(auth_state): Extension<AuthState>,
    context: StaffContext,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<IssueControlLicenseRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<IssuedLicenseResponse>), HttpError> {
    crate::control::require_origin(&headers, &auth_state)?;
    if !state.license_issuance_enabled {
        return Err(err(AppError::LicensingDisabled));
    }
    let Path(installation_id) = path.map_err(path_error)?;
    let installation_id = InstallationId::from_uuid(installation_id);
    let Json(request) = request.map_err(json_error)?;
    let product_id = ProductId::from_uuid(request.product_id);

    let installation = state
        .service
        .get_installation(installation_id)
        .await
        .map_err(issuance_error)?;

    let actor = actor(&auth_state, &context).await?;
    // Rechazo temprano con recurso conocido: sin capacidad de gestionar
    // instalaciones no se consulta ningún derecho comercial.
    if !actor.has_capability(Capability::InstallationsManage) {
        audit_denial(
            &state,
            installation_id,
            installation.organization_id,
            product_id,
            &actor,
            "capability",
        )
        .await;
        return Err(issuance_error(AppError::Unauthorized));
    }

    let now = time::OffsetDateTime::now_utc();
    let issued = match resolve_entitlement(
        &state,
        &actor,
        installation.organization_id,
        product_id,
        now,
    )
    .await
    {
        Ok(entitled) => {
            let result = state
                .service
                .issue_entitled_license(installation_id, entitled, &actor, now)
                .await;
            match result {
                Ok(view) => Ok(view),
                Err(error) => {
                    if matches!(
                        error,
                        AppError::Unauthorized
                            | AppError::NotEntitled
                            | AppError::UnsupportedPlan
                            | AppError::UnsupportedFeature
                            | AppError::Conflict
                    ) {
                        audit_denial(
                            &state,
                            installation_id,
                            installation.organization_id,
                            product_id,
                            &actor,
                            denial_reason(&error),
                        )
                        .await;
                    }
                    Err(error)
                }
            }
        }
        Err((error, reason)) => {
            if let Some(reason) = reason {
                audit_denial(
                    &state,
                    installation_id,
                    installation.organization_id,
                    product_id,
                    &actor,
                    reason,
                )
                .await;
            }
            Err(error)
        }
    }
    .map_err(issuance_error)?;

    Ok((StatusCode::CREATED, Json(issued.into())))
}

/// Resuelve el derecho comercial de la organización sobre el producto.
///
/// Devuelve el error de aplicación junto con la causa estable para auditar el
/// rechazo cuando procede.
async fn resolve_entitlement(
    state: &AppState,
    actor: &CommercialActor,
    organization_id: OrganizationId,
    product_id: ProductId,
    now: time::OffsetDateTime,
) -> Result<EntitledPlan, (AppError, Option<&'static str>)> {
    use platform_application::commercial_domain::CommercialError;

    let entitlements = state
        .commercial_domain
        .entitlements(actor, organization_id, product_id, now)
        .await;
    let entitlements = match entitlements {
        Ok(value) => value,
        Err(CommercialError::Forbidden) => {
            return Err((AppError::Unauthorized, Some("capability")));
        }
        Err(CommercialError::NotFound) => return Err((AppError::NotFound, None)),
        Err(_) => return Err((AppError::Storage, None)),
    };
    EntitledPlan::from_entitlements(organization_id, product_id.as_uuid(), &entitlements).map_err(
        |error| match error {
            AppError::NotEntitled => (AppError::NotEntitled, Some("not_entitled")),
            AppError::UnsupportedPlan => (AppError::UnsupportedPlan, Some("unsupported_plan")),
            AppError::UnsupportedFeature => {
                (AppError::UnsupportedFeature, Some("unsupported_feature"))
            }
            other => (other, None),
        },
    )
}

fn denial_reason(error: &AppError) -> &'static str {
    match error {
        AppError::Unauthorized => "capability",
        AppError::NotEntitled => "not_entitled",
        AppError::UnsupportedPlan => "unsupported_plan",
        AppError::UnsupportedFeature => "unsupported_feature",
        AppError::Conflict => "duplicate",
        _ => "unknown",
    }
}
