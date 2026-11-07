//! API HTTP de HiQNet Platform.
//!
//! Seguridad: las operaciones de instalaciones y licencias exigen un token
//! administrativo DEV (`HIQNET_PLATFORM_ADMIN_TOKEN`). Sin él, se rechazan con 401.

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
mod contract;
use contract::{
    ErrorResponse, HealthResponse, InstallationResponse, IssueLicenseBody, IssuedLicenseResponse,
    LicenseRecordResponse, PlanResponse, ReadinessResponse, RegisterInstallationBody,
};
use platform_application::{AppError, PlatformService, SharedRepository, SharedSigner};
use platform_core::DomainError;
use platform_core::ids::InstallationId;
use platform_storage_postgres::{is_ready, open, pool_repository};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::openapi::{
    Info, OpenApi as OpenApiDocument,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<PlatformService>,
    pub admin_token: Option<String>,
    pub licensing_enabled: bool,
    pub db_pool: PgPool,
}

fn err(error: AppError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        AppError::NotFound | AppError::Domain(DomainError::InstallationNotFound) => {
            StatusCode::NOT_FOUND
        }
        AppError::Unauthorized | AppError::AdminDisabled => StatusCode::UNAUTHORIZED,
        AppError::LicensingDisabled
        | AppError::Storage
        | AppError::Domain(DomainError::LicensingDisabled) => StatusCode::SERVICE_UNAVAILABLE,
        AppError::Conflict
        | AppError::Domain(
            DomainError::DuplicateInstallation | DomainError::TrialAlreadyActivated,
        ) => StatusCode::CONFLICT,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(ErrorResponse {
            code: error.code(),
            message: error.to_string(),
        }),
    )
}

type HttpError = (StatusCode, Json<ErrorResponse>);

fn json_error(rejection: JsonRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_request",
            message: "cuerpo JSON inválido o incompatible con el contrato".into(),
        }),
    )
}

fn path_error(rejection: PathRejection) -> HttpError {
    (
        rejection.status(),
        Json(ErrorResponse {
            code: "invalid_path",
            message: "identificador de ruta inválido".into(),
        }),
    )
}

fn path_id(path: Result<Path<Uuid>, PathRejection>) -> Result<InstallationId, HttpError> {
    path.map(|Path(id)| InstallationId::from_uuid(id))
        .map_err(path_error)
}

fn admin_guard(headers: &HeaderMap, token: &Option<String>) -> Result<(), AppError> {
    let Some(expected) = token else {
        return Err(AppError::AdminDisabled);
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(expected.as_str()) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn api_routes() -> (Router<AppState>, OpenApiDocument) {
    let (router, mut document) = OpenApiRouter::<AppState>::new()
        .routes(routes!(health))
        .routes(routes!(ready))
        .routes(routes!(list_plans))
        .routes(routes!(register_installation))
        .routes(routes!(get_installation))
        .routes(routes!(start_trial))
        .routes(routes!(issue_license, list_licenses))
        .split_for_parts();
    document.info = Info::new("HiQNet Platform API", env!("CARGO_PKG_VERSION"));
    document.info.description = Some("Plano de control técnico; sin identidad comercial, facturación ni sincronización productivas. El token administrativo es temporal y exclusivo de desarrollo.".into());
    document
        .components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
        "devAdminToken",
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .description(Some(
                    "Token administrativo temporal para pruebas DEV; no es una sesión de usuario.",
                ))
                .build(),
        ),
    );
    (router, document)
}

pub fn openapi_document() -> OpenApiDocument {
    api_routes().1
}

pub fn build_router(state: AppState, docs_enabled: bool) -> Router {
    let (router, document) = api_routes();
    let router = router.with_state(state);
    if docs_enabled {
        router.merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", document))
    } else {
        router
    }
}

/// Comprueba únicamente que el proceso HTTP responde; no implica disponibilidad de PostgreSQL.
#[utoipa::path(get, path = "/health", operation_id = "getHealth", tag = "Operación",
    responses((status = 200, description = "Proceso HTTP activo; no comprueba PostgreSQL", body = HealthResponse,
        example = json!({"status":"ok","version":"0.1.0"}))))]
async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Ejecuta un roundtrip real a PostgreSQL para determinar si la API puede atender tráfico.
#[utoipa::path(get, path = "/ready", operation_id = "getReadiness", tag = "Operación",
    responses(
        (status = 200, description = "PostgreSQL respondió a SELECT 1", body = ReadinessResponse,
            example = json!({"status":"ok","database":"up"})),
        (status = 503, description = "PostgreSQL no disponible", body = ReadinessResponse,
            example = json!({"status":"unavailable","database":"down"}))))]
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    if is_ready(&state.db_pool).await {
        (
            StatusCode::OK,
            Json(ReadinessResponse {
                status: "ok",
                database: "up",
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                status: "unavailable",
                database: "down",
            }),
        )
    }
}

/// Devuelve el catálogo técnico. Los nombres de plan no implican disponibilidad comercial.
#[utoipa::path(get, path = "/api/v1/plans", operation_id = "listPlans", tag = "Planes",
    responses(
        (status = 200, description = "Catálogo de planes", body = [PlanResponse],
            example = json!([{"plan":"trial","display_name":"Trial (30 días)","features":["catalog","sales"]}]))))]
async fn list_plans(State(state): State<AppState>) -> Result<Json<Vec<PlanResponse>>, HttpError> {
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
async fn register_installation(
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
async fn get_installation(
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
async fn start_trial(
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
async fn issue_license(
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
async fn list_licenses(
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

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = open(&database_url).await?;
    let repository: SharedRepository = Arc::new(pool_repository(pool.clone()));

    let signing_key = std::env::var("HIQNET_PLATFORM_SIGNING_KEY_B64")
        .ok()
        .filter(|value| !value.is_empty());
    let signer = signing_key
        .as_deref()
        .map(platform_licensing::Ed25519Signer::from_seed_base64)
        .transpose()
        .map_err(|error| anyhow::anyhow!(error))?
        .map(|signer| Arc::new(signer) as SharedSigner);

    let licensing_enabled = signer.is_some()
        && std::env::var("HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE")
            .map(|value| value == "true")
            .unwrap_or(false);

    let admin_token = std::env::var("HIQNET_PLATFORM_ADMIN_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());

    let service = Arc::new(PlatformService::new(repository, signer));
    let state = AppState {
        service,
        admin_token,
        licensing_enabled,
        db_pool: pool,
    };

    // La documentación sólo se expone cuando el entorno la habilita explícitamente.
    let docs_enabled =
        std::env::var("HIQNET_PLATFORM_ENABLE_DOCS").is_ok_and(|value| value == "true");
    let app = build_router(state, docs_enabled);
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "HiQNet Platform API listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, header},
    };
    use base64::Engine as _;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tower::ServiceExt;

    fn isolated_test_url() -> String {
        let url = std::env::var("TEST_DATABASE_URL").expect("base aislada requerida");
        assert!(
            url.rsplit('/')
                .next()
                .is_some_and(|database| database.contains("test") && database != "hiqnet"),
            "TEST_DATABASE_URL debe identificar una base aislada llamada test"
        );
        url
    }

    fn state(pool: PgPool) -> AppState {
        AppState {
            service: Arc::new(PlatformService::new(
                Arc::new(pool_repository(pool.clone())),
                None,
            )),
            admin_token: None,
            licensing_enabled: false,
            db_pool: pool,
        }
    }

    fn dev_state(pool: PgPool, licensing_enabled: bool) -> AppState {
        // Semilla efímera para pruebas; nunca se almacena ni se publica.
        let mut seed = [0_u8; 32];
        seed[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        seed[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        let signer: SharedSigner = Arc::new(
            platform_licensing::Ed25519Signer::from_seed_base64(
                &base64::engine::general_purpose::STANDARD.encode(seed),
            )
            .unwrap(),
        );
        AppState {
            service: Arc::new(PlatformService::new(
                Arc::new(pool_repository(pool.clone())),
                Some(signer),
            )),
            admin_token: Some("token-sintetico-p01".into()),
            licensing_enabled,
            db_pool: pool,
        }
    }

    async fn test_organization(pool: &PgPool, id: Uuid) {
        sqlx::query(
            "INSERT INTO organization (id, display_name, status, created_at)
             VALUES ($1, 'Organización sintética', 'active', now())",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn remove_test_organization(pool: &PgPool, id: Uuid) {
        sqlx::query("DELETE FROM organization WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn send(
        router: &Router,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let payload = if let Some(body) = body {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };
        let response = router
            .clone()
            .oneshot(request.body(payload).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    #[test]
    fn generated_contract_matches_versioned_snapshot_and_expected_routes() {
        let current = serde_json::to_value(openapi_document()).unwrap();
        let saved: Value =
            serde_json::from_str(include_str!("../../../openapi/openapi.json")).unwrap();
        assert_eq!(
            current, saved,
            "regenera con cargo run -p hiqnet-platform-api --bin generate-openapi"
        );
        assert_eq!(current["openapi"], "3.1.0");
        let expected = [
            ("/health", "get"),
            ("/ready", "get"),
            ("/api/v1/plans", "get"),
            ("/api/v1/installations", "post"),
            ("/api/v1/installations/{id}", "get"),
            ("/api/v1/installations/{id}/trial", "post"),
            ("/api/v1/installations/{id}/licenses", "get"),
            ("/api/v1/installations/{id}/licenses", "post"),
        ];
        let actual: Vec<(&str, &str)> = current["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(path, operations)| {
                operations
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(move |method| (path.as_str(), method.as_str()))
            })
            .collect();
        assert_eq!(actual.len(), expected.len());
        for (path, method) in expected {
            let operation = &current["paths"][path][method];
            assert!(operation.is_object(), "ruta {method} {path} ausente");
            assert!(operation["operationId"].is_string());
            assert!(operation["summary"].is_string());
            assert!(
                operation["responses"]
                    .as_object()
                    .is_some_and(|responses| !responses.is_empty())
            );
            if path.starts_with("/api/v1/installations") {
                assert_eq!(operation["security"], json!([{"devAdminToken": []}]));
            } else {
                assert!(operation.get("security").is_none());
            }
        }
    }

    #[tokio::test]
    async fn swagger_is_opt_in_and_serves_generated_document() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        let disabled = build_router(state(pool.clone()), false);
        assert_eq!(
            send(&disabled, Method::GET, "/docs", None, None).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&disabled, Method::GET, "/api-docs/openapi.json", None, None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let enabled = build_router(state(pool), true);
        let spec = send(&enabled, Method::GET, "/api-docs/openapi.json", None, None).await;
        assert_eq!(spec.0, StatusCode::OK);
        assert_eq!(spec.1, serde_json::to_value(openapi_document()).unwrap());
        let redirect = enabled
            .clone()
            .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(redirect.status().is_redirection());
        assert_eq!(redirect.headers()[header::LOCATION], "/docs/");
        let ui = enabled
            .oneshot(
                Request::builder()
                    .uri("/docs/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ui.status(), StatusCode::OK);
        let html = to_bytes(ui.into_body(), 1024 * 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&html).contains("Swagger UI"));
    }

    #[tokio::test]
    async fn http_routes_auth_errors_and_real_postgres_match_contract() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        let router = build_router(dev_state(pool.clone(), false), true);
        let id = Uuid::new_v4();
        let org = Uuid::new_v4();
        test_organization(&pool, org).await;
        let path = format!("/api/v1/installations/{id}");
        let body = json!({"installation_id":id,"organization_id":org,"device_label":format!("fixture-{id}")});
        let token = Some("token-sintetico-p01");

        assert_eq!(
            send(&router, Method::GET, "/health", None, None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, "/ready", None, None).await.0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, "/api/v1/plans", None, None)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            send(&router, Method::GET, &path, None, None).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&router, Method::POST, &format!("{path}/trial"), None, None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &format!("{path}/licenses"),
                None,
                Some(json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&router, Method::GET, &path, Some("incorrecto"), None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                None,
                Some(body.clone())
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );

        let registered = send(
            &router,
            Method::POST,
            "/api/v1/installations",
            token,
            Some(body.clone()),
        )
        .await;
        assert_eq!(registered.0, StatusCode::CREATED);
        assert_eq!(registered.1["id"], id.to_string());
        assert_eq!(
            send(&router, Method::GET, &path, token, None).await.0,
            StatusCode::OK
        );
        let missing = send(
            &router,
            Method::GET,
            &format!("/api/v1/installations/{}", Uuid::new_v4()),
            token,
            None,
        )
        .await;
        assert_eq!(missing.0, StatusCode::NOT_FOUND);
        assert_eq!(missing.1["code"], "not_found");
        assert_eq!(
            send(
                &router,
                Method::GET,
                "/api/v1/installations/bad-uuid",
                token,
                None
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &router,
                Method::GET,
                &format!("{path}/licenses"),
                None,
                None
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &router,
                Method::GET,
                &format!("{path}/licenses"),
                token,
                None
            )
            .await
            .1,
            json!([])
        );

        let trial = send(&router, Method::POST, &format!("{path}/trial"), token, None).await;
        assert_eq!(trial.0, StatusCode::OK);
        assert!(trial.1["trial_ends_at"].is_string());
        assert_eq!(
            send(&router, Method::POST, &format!("{path}/trial"), token, None)
                .await
                .0,
            StatusCode::CONFLICT
        );
        let duplicate = json!({"installation_id":Uuid::new_v4(),"organization_id":org,"device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(duplicate)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let other_org = Uuid::new_v4();
        test_organization(&pool, other_org).await;
        let wrong_org = json!({"installation_id":id,"organization_id":other_org,"device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(wrong_org)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        remove_test_organization(&pool, other_org).await;
        let invalid = json!({"organization_id":org,"device_label":"   "});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(invalid)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let invalid_type = json!({"organization_id":org,"device_label":99});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(invalid_type)
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let issue = json!({"plan":"trial","valid_until":"2030-01-01T00:00:00Z"});
        assert_eq!(
            send(
                &router,
                Method::POST,
                &format!("{path}/licenses"),
                token,
                Some(issue)
            )
            .await
            .0,
            StatusCode::SERVICE_UNAVAILABLE
        );

        // La fixture sólo existe en TEST_DATABASE_URL; no altera la base principal.
        sqlx::query("DELETE FROM installation WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        remove_test_organization(&pool, org).await;
    }

    #[tokio::test]
    async fn down_database_returns_normalized_error_without_sql() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        let router = build_router(dev_state(pool, false), false);
        let path = format!("/api/v1/installations/{}", Uuid::new_v4());
        let (status, error) = send(
            &router,
            Method::GET,
            &path,
            Some("token-sintetico-p01"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error["code"], "storage_error");
        assert!(!error.to_string().contains("SELECT"));
        assert!(!error.to_string().contains("postgres://"));
    }

    #[tokio::test]
    async fn dev_signing_and_json_rejections_match_documented_responses() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        let router = build_router(dev_state(pool.clone(), true), false);
        let id = Uuid::new_v4();
        let org = Uuid::new_v4();
        test_organization(&pool, org).await;
        let token = Some("token-sintetico-p01");
        let path = format!("/api/v1/installations/{id}/licenses");
        let registration = json!({"installation_id": id, "organization_id": org, "device_label":format!("fixture-{id}")});
        assert_eq!(
            send(
                &router,
                Method::POST,
                "/api/v1/installations",
                token,
                Some(registration)
            )
            .await
            .0,
            StatusCode::CREATED
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &path,
                token,
                Some(json!({"plan":"unknown", "valid_until":"2030-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &router,
                Method::POST,
                &path,
                token,
                Some(json!({"plan":"trial", "valid_until":"2000-01-01T00:00:00Z"}))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let issued = send(
            &router,
            Method::POST,
            &path,
            token,
            Some(json!({"plan":"trial", "valid_until":"2030-01-01T00:00:00Z"})),
        )
        .await;
        assert_eq!(issued.0, StatusCode::CREATED);
        assert!(issued.1["grant_json"].is_string());
        assert!(issued.1["signature_b64"].is_string());
        let list = send(&router, Method::GET, &path, token, None).await;
        assert_eq!(list.0, StatusCode::OK);
        assert_eq!(list.1[0]["grant_id"], issued.1["grant_id"]);
        assert!(list.1[0]["features_json"].is_string());

        let absent_type = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/installations")
                    .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(absent_type.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let malformed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/installations")
                    .header(header::AUTHORIZATION, "Bearer token-sintetico-p01")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert!(
            malformed
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json")
        );
        let cross_origin = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header(header::ORIGIN, "https://otro.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            cross_origin
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );

        sqlx::query("DELETE FROM issued_license WHERE installation_id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM installation WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        remove_test_organization(&pool, org).await;
    }

    #[tokio::test]
    async fn liveness_does_not_claim_database_health_when_database_is_down() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(300))
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
            .unwrap();
        assert_eq!(health().await.into_response().status(), StatusCode::OK);
        assert_eq!(
            ready(State(state(pool))).await.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn readiness_uses_real_postgres() {
        let url = isolated_test_url();
        let pool = open(&url).await.unwrap();
        assert_eq!(
            ready(State(state(pool))).await.into_response().status(),
            StatusCode::OK
        );
    }

    #[test]
    fn storage_errors_are_not_client_errors() {
        assert_eq!(err(AppError::Storage).0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn no_admin_token_rejects_even_empty_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer ".parse().unwrap(),
        );
        assert!(matches!(
            admin_guard(&headers, &None),
            Err(AppError::AdminDisabled)
        ));
    }
}
