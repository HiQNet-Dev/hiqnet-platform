//! Liveness y readiness del proceso.

use crate::*;

/// Comprueba únicamente que el proceso HTTP responde; no implica disponibilidad de PostgreSQL.
#[utoipa::path(get, path = "/health", operation_id = "getHealth", tag = "Operación",
    responses((status = 200, description = "Proceso HTTP activo; no comprueba PostgreSQL", body = HealthResponse,
        example = json!({"status":"ok","version":"0.1.0"}))))]
pub(crate) async fn health() -> impl IntoResponse {
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
pub(crate) async fn ready(State(state): State<AppState>) -> impl IntoResponse {
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
