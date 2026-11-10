//! Endpoints HTTP de enrolamiento; los handlers solo traducen contratos.

use crate::{
    AppState, auth,
    auth::{AuthContext, AuthState},
    contract::ErrorResponse,
    json_error, path_error, query_error,
};
use axum::{
    Json,
    extract::{
        ConnectInfo, FromRequestParts, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, request::Parts},
};
use platform_application::enrollment::{
    ApproveEnrollmentRequest, BeginEnrollmentRequest, EnrollmentError, ProofAttempt,
};
use platform_core::ids::{InstallationId, OrganizationId};
use serde::{Deserialize, Serialize};
use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

type HttpError = (StatusCode, Json<ErrorResponse>);

/// Peer de la conexión TCP. En producción Axum la inyecta mediante
/// `into_make_service_with_connect_info`; no se leen cabeceras de proxy.
#[derive(Clone, Copy)]
pub(crate) struct PeerIp(Option<IpAddr>);

impl PeerIp {
    #[cfg(test)]
    pub(crate) fn for_test(ip: IpAddr) -> Self {
        Self(Some(ip))
    }
}

impl<S> FromRequestParts<S> for PeerIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Las pruebas inyectan el peer directamente; producción usa ConnectInfo.
        if let Some(peer) = parts.extensions.get::<PeerIp>() {
            return Ok(*peer);
        }
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|connect| connect.0.ip()),
        ))
    }
}

impl PeerIp {
    fn rate_key(&self, scope: &str) -> String {
        let peer = self
            .0
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown-peer".into());
        format!("enrollment:{scope}:{peer}")
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginEnrollmentBody {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174000")]
    pub installation_id: Uuid,
    #[schema(example = "pos")]
    pub product_key: String,
    #[schema(example = "ed25519:v1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")]
    pub public_key: String,
    #[schema(example = "Caja mostrador")]
    pub device_label: String,
    #[schema(example = "0.1.0")]
    pub app_version: String,
    #[schema(example = "windows")]
    pub platform: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollmentChallengeResponse {
    #[schema(example = "sintetico-device-code")]
    pub device_code: String,
    #[schema(example = "ABCD-EFGH")]
    pub user_code: String,
    #[schema(example = "http://127.0.0.1:8080/device")]
    pub verification_uri: String,
    #[schema(example = "sintetico-nonce")]
    pub nonce: String,
    #[schema(example = 600)]
    pub expires_in: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApproveEnrollmentBody {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174010")]
    pub organization_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApprovedEnrollmentResponse {
    #[schema(value_type = String, format = Uuid)]
    pub enrollment_request_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: Uuid,
    #[schema(example = "approved")]
    pub status: &'static str,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProofBody {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174000")]
    pub installation_id: Uuid,
    #[schema(example = "ed25519:v1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")]
    pub public_key: String,
    #[schema(example = "sintetico-nonce")]
    pub nonce: String,
    #[schema(example = "firma-sintetica-base64url")]
    pub signature: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollmentCompletedResponse {
    #[schema(value_type = String, format = Uuid)]
    pub enrollment_request_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_identity_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_binding_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub bound_at: time::OffsetDateTime,
    #[schema(example = "completed")]
    pub status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct InstallationsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstallationListItemResponse {
    #[schema(value_type = String, format = Uuid)]
    pub installation_id: Uuid,
    pub device_label: String,
    pub status: String,
    pub identity_status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub bound_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InstallationsResponse {
    pub items: Vec<InstallationListItemResponse>,
    pub next_cursor: Option<String>,
}

pub fn enrollment_error(error: EnrollmentError) -> HttpError {
    match error {
        EnrollmentError::Invalid | EnrollmentError::Domain(_) => auth::error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "solicitud de enrolamiento inválida",
        ),
        EnrollmentError::NotFound => auth::error(
            StatusCode::NOT_FOUND,
            "not_found",
            "recurso de enrolamiento no encontrado",
        ),
        EnrollmentError::Forbidden => auth::error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "autoridad comercial insuficiente",
        ),
        EnrollmentError::EnrollmentInvalid => auth::error(
            StatusCode::CONFLICT,
            "enrollment_invalid",
            "la aprobación de enrolamiento dejó de ser válida",
        ),
        EnrollmentError::Conflict => auth::error(
            StatusCode::CONFLICT,
            "enrollment_conflict",
            "conflicto de identidad o vinculación",
        ),
        EnrollmentError::RestartRequired => auth::error(
            StatusCode::CONFLICT,
            "enrollment_restart_required",
            "el challenge ya fue entregado; inicia un flujo nuevo",
        ),
        EnrollmentError::Expired => auth::error(
            StatusCode::GONE,
            "enrollment_expired",
            "el challenge expiró o fue consumido",
        ),
        EnrollmentError::InvalidProof => auth::error(
            StatusCode::BAD_REQUEST,
            "invalid_proof",
            "la prueba de posesión no es válida",
        ),
        EnrollmentError::IdempotencyConflict => auth::error(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "la Idempotency-Key se reutilizó con un payload distinto",
        ),
        EnrollmentError::Storage => auth::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_error",
            "no fue posible completar la operación",
        ),
    }
}

fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, HttpError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| {
        auth::error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
            "Idempotency-Key inválida",
        )
    })?;
    if value.is_empty() || value.len() > platform_core::enrollment::MAX_IDEMPOTENCY_KEY_LEN {
        return Err(auth::error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
            "Idempotency-Key inválida",
        ));
    }
    Ok(Some(value.to_owned()))
}

/// Inicia un challenge de enrolamiento público; los secretos se entregan una sola vez.
#[utoipa::path(post, path = "/api/v1/enrollment-requests", operation_id = "beginEnrollment", tag = "Enrollment",
    params(("Idempotency-Key" = Option<String>, Header, description = "Clave de idempotencia; máximo 128 caracteres ASCII visibles")),
    request_body(content = BeginEnrollmentBody, description = "Identidad pública, product_key resuelto por Platform y metadatos no secretos"),
    responses(
        (status = 201, description = "Challenge creado; secretos entregados una sola vez", body = EnrollmentChallengeResponse),
        (status = 400, description = "Payload, clave pública, plataforma o Idempotency-Key inválidos", body = ErrorResponse),
        (status = 409, description = "Reinicio requerido o conflicto de idempotencia", body = ErrorResponse),
        (status = 429, description = "Rate limit técnico de enrolamiento", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn begin_enrollment(
    State(state): State<AppState>,
    peer: PeerIp,
    headers: HeaderMap,
    request: Result<Json<BeginEnrollmentBody>, JsonRejection>,
) -> Result<(StatusCode, Json<EnrollmentChallengeResponse>), HttpError> {
    let rate_key = peer.rate_key("begin");
    if state.enrollment_begin_limiter.is_limited(&rate_key) {
        return Err(auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiados intentos de enrolamiento",
        ));
    }
    state.enrollment_begin_limiter.record(&rate_key);
    let idempotency_key = idempotency_key(&headers)?;
    let Json(request) = request.map_err(json_error)?;
    let challenge = state
        .enrollment
        .begin_enrollment(
            BeginEnrollmentRequest {
                installation_id: InstallationId::from_uuid(request.installation_id),
                product_key: request.product_key,
                public_key: request.public_key,
                device_label: request.device_label,
                app_version: request.app_version,
                platform: request.platform,
                idempotency_key,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(enrollment_error)?;
    Ok((
        StatusCode::CREATED,
        Json(EnrollmentChallengeResponse {
            device_code: challenge.device_code,
            user_code: challenge.user_code,
            verification_uri: challenge.verification_uri,
            nonce: challenge.nonce,
            expires_in: challenge.expires_in,
        }),
    ))
}

/// Aprueba un challenge desde una sesión comercial; sólo el owner fija el tenant.
#[utoipa::path(post, path = "/api/v1/enrollment-requests/{user_code}/approve", operation_id = "approveEnrollment", tag = "Enrollment",
    params(("user_code" = String, Path, description = "Código de usuario mostrado al POS")),
    request_body(content = ApproveEnrollmentBody, description = "Organización propietaria elegida por el owner"),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Solicitud aprobada", body = ApprovedEnrollmentResponse),
        (status = 400, description = "Código, organización o JSON inválidos", body = ErrorResponse),
        (status = 401, description = "Sesión inválida", body = ErrorResponse),
        (status = 403, description = "Origen no permitido o rol distinto de owner", body = ErrorResponse),
        (status = 404, description = "Código, organización o membresía inexistente (neutral)", body = ErrorResponse),
        (status = 410, description = "Challenge expirado", body = ErrorResponse),
        (status = 429, description = "Rate limit técnico por cuenta", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn approve_enrollment(
    State(state): State<AppState>,
    axum::Extension(auth_state): axum::Extension<AuthState>,
    auth_context: AuthContext,
    headers: HeaderMap,
    user_code: Result<Path<String>, PathRejection>,
    request: Result<Json<ApproveEnrollmentBody>, JsonRejection>,
) -> Result<Json<ApprovedEnrollmentResponse>, HttpError> {
    let rate_key = format!(
        "enrollment:approve:{}",
        auth_context.session.account_id.as_uuid()
    );
    if state.enrollment_approve_limiter.is_limited(&rate_key) {
        return Err(auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiadas aprobaciones de enrolamiento",
        ));
    }
    state.enrollment_approve_limiter.record(&rate_key);
    if !auth::origin_allowed(&headers, &auth_state) {
        return Err(auth::error(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "origen no permitido",
        ));
    }
    let Path(user_code) = user_code.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let approved = state
        .enrollment
        .approve_enrollment(
            ApproveEnrollmentRequest {
                user_code,
                organization_id: OrganizationId::from_uuid(request.organization_id),
                actor_account_id: auth_context.session.account_id,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(enrollment_error)?;
    Ok(Json(ApprovedEnrollmentResponse {
        enrollment_request_id: approved.request_id.as_uuid(),
        installation_id: approved.installation_id.as_uuid(),
        organization_id: approved.organization_id.as_uuid(),
        status: "approved",
    }))
}

/// Verifica la firma Ed25519 del dispositivo y consume el challenge una sola vez.
#[utoipa::path(post, path = "/api/v1/enrollment-requests/{device_code}/proof", operation_id = "proveEnrollment", tag = "Enrollment",
    params(("device_code" = String, Path, description = "Código largo entregado al POS")),
    request_body(content = ProofBody, description = "Firma sobre el mensaje canónico hiqnet-enrollment-proof:v1"),
    responses(
        (status = 201, description = "Instalación vinculada", body = EnrollmentCompletedResponse),
        (status = 400, description = "Firma o payload inválidos", body = ErrorResponse),
        (status = 404, description = "Challenge inexistente", body = ErrorResponse),
        (status = 409, description = "Replay, conflicto o enrollment_invalid si la autoridad decayó", body = ErrorResponse),
        (status = 410, description = "Challenge expirado o consumido", body = ErrorResponse),
        (status = 429, description = "Rate limit técnico por peer", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn prove_enrollment(
    State(state): State<AppState>,
    peer: PeerIp,
    device_code: Result<Path<String>, PathRejection>,
    request: Result<Json<ProofBody>, JsonRejection>,
) -> Result<(StatusCode, Json<EnrollmentCompletedResponse>), HttpError> {
    let rate_key = peer.rate_key("proof");
    if state.enrollment_proof_limiter.is_limited(&rate_key) {
        return Err(auth::error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiados intentos de proof",
        ));
    }
    state.enrollment_proof_limiter.record(&rate_key);
    let Path(device_code) = device_code.map_err(path_error)?;
    let Json(request) = request.map_err(json_error)?;
    let completion = state
        .enrollment
        .prove_enrollment(
            ProofAttempt {
                device_code,
                installation_id: InstallationId::from_uuid(request.installation_id),
                public_key: request.public_key,
                nonce: request.nonce,
                signature: request.signature,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(enrollment_error)?;
    Ok((
        StatusCode::CREATED,
        Json(EnrollmentCompletedResponse {
            enrollment_request_id: completion.enrollment_request_id.as_uuid(),
            installation_id: completion.installation_id.as_uuid(),
            organization_id: completion.organization_id.as_uuid(),
            installation_identity_id: completion.identity_id,
            installation_binding_id: completion.binding_id,
            bound_at: completion.bound_at,
            status: "completed",
        }),
    ))
}

/// Lista installations de una organización con aislamiento en SQL y cursor keyset.
#[utoipa::path(get, path = "/api/v1/organizations/{id}/installations", operation_id = "listOrganizationInstallations", tag = "Installations",
    params(
        ("id" = Uuid, Path, description = "Identificador UUID de la organización"),
        ("limit" = Option<u32>, Query, description = "Tamaño de página, 1–100 (default 50)"),
        ("cursor" = Option<String>, Query, description = "Cursor opaco keyset ligado a la organización")),
    security(("commercialSession" = [])),
    responses(
        (status = 200, description = "Listado de installations visibles para el actor", body = InstallationsResponse),
        (status = 400, description = "UUID, cursor o límite inválidos", body = ErrorResponse),
        (status = 401, description = "Sesión inválida", body = ErrorResponse),
        (status = 403, description = "Origen no permitido", body = ErrorResponse),
        (status = 404, description = "Organización ajena o inexistente (neutral)", body = ErrorResponse),
        (status = 503, description = "PostgreSQL no disponible", body = ErrorResponse)))]
pub async fn list_installations(
    State(state): State<AppState>,
    auth_context: AuthContext,
    id: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<InstallationsQuery>, QueryRejection>,
) -> Result<Json<InstallationsResponse>, HttpError> {
    let Path(id) = id.map_err(path_error)?;
    let Query(query) = query.map_err(query_error)?;
    let page = state
        .enrollment
        .list_installations(
            auth_context.session.account_id,
            OrganizationId::from_uuid(id),
            query.cursor.as_deref(),
            query.limit.unwrap_or(50),
        )
        .await
        .map_err(enrollment_error)?;
    Ok(Json(InstallationsResponse {
        items: page
            .items
            .into_iter()
            .map(|item| InstallationListItemResponse {
                installation_id: item.installation_id.as_uuid(),
                device_label: item.device_label,
                status: item.status,
                identity_status: item.identity_status,
                bound_at: item.bound_at,
                last_seen_at: item.last_seen_at,
            })
            .collect(),
        next_cursor: page.next_cursor,
    }))
}
