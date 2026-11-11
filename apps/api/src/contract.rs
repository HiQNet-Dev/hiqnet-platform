//! DTO HTTP: contrato público independiente de los tipos de persistencia y del dominio.

use platform_application::{
    InstallationView, IssueLicenseRequest, IssuedLicenseView, PlanView, RegisterInstallationRequest,
};
use platform_core::license::IssuedLicense;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(example = "ok")]
    pub status: &'static str,
    #[schema(example = "0.1.0")]
    pub version: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ReadinessResponse {
    #[schema(example = "ok")]
    pub status: &'static str,
    #[schema(example = "up")]
    pub database: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    #[schema(example = "not_found")]
    pub code: &'static str,
    #[schema(example = "no se encontró el recurso solicitado")]
    pub message: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterInstallationBody {
    #[schema(value_type = Option<String>, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174000")]
    pub installation_id: Option<Uuid>,
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174001")]
    pub organization_id: Uuid,
    #[schema(min_length = 1, example = "Caja de pruebas")]
    pub device_label: String,
}

impl From<RegisterInstallationBody> for RegisterInstallationRequest {
    fn from(value: RegisterInstallationBody) -> Self {
        Self {
            installation_id: value.installation_id,
            organization_id: value.organization_id,
            device_label: value.device_label,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueLicenseBody {
    #[schema(example = "trial")]
    pub plan: String,
    #[schema(example = "2030-01-01T00:00:00Z")]
    #[serde(with = "time::serde::rfc3339")]
    pub valid_until: OffsetDateTime,
}

impl From<IssueLicenseBody> for IssueLicenseRequest {
    fn from(value: IssueLicenseBody) -> Self {
        Self {
            plan: value.plan,
            valid_until: value.valid_until,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct PlanResponse {
    #[schema(example = "trial")]
    pub plan: String,
    #[schema(example = "Trial (30 días)")]
    pub display_name: String,
    #[schema(example = json!(["catalog", "sales"]))]
    pub features: Vec<String>,
}

impl From<PlanView> for PlanResponse {
    fn from(value: PlanView) -> Self {
        Self {
            plan: value.plan,
            display_name: value.display_name,
            features: value.features,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct InstallationResponse {
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174000")]
    pub id: Uuid,
    #[schema(value_type = String, format = Uuid, example = "123e4567-e89b-42d3-a456-426614174001")]
    pub organization_id: Uuid,
    #[schema(example = "Caja de pruebas")]
    pub device_label: String,
    #[schema(example = "active")]
    pub status: &'static str,
    #[serde(with = "time::serde::rfc3339")]
    pub registered_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub trial_started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub trial_ends_at: Option<OffsetDateTime>,
}

impl From<InstallationView> for InstallationResponse {
    fn from(value: InstallationView) -> Self {
        Self {
            id: value.id.as_uuid(),
            organization_id: value.organization_id.as_uuid(),
            device_label: value.device_label,
            status: value.status,
            registered_at: value.registered_at,
            trial_started_at: value.trial_started_at,
            trial_ends_at: value.trial_ends_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct IssuedLicenseResponse {
    #[schema(value_type = String, format = Uuid)]
    pub grant_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_id: Uuid,
    pub plan: String,
    #[serde(with = "time::serde::rfc3339")]
    pub valid_until: OffsetDateTime,
    pub grant_json: String,
    pub signature_b64: String,
    pub public_key_b64: String,
    /// true cuando la respuesta reentrega el grant ya emitido (recuperación),
    /// false cuando es una emisión nueva.
    pub recovered: bool,
}

impl From<IssuedLicenseView> for IssuedLicenseResponse {
    fn from(value: IssuedLicenseView) -> Self {
        Self {
            grant_id: value.grant_id.as_uuid(),
            installation_id: value.installation_id.as_uuid(),
            plan: value.plan,
            valid_until: value.valid_until,
            grant_json: value.grant_json,
            signature_b64: value.signature_b64,
            public_key_b64: value.public_key_b64,
            recovered: value.recovered,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LicenseRecordResponse {
    #[schema(value_type = String, format = Uuid)]
    pub grant_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub installation_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub organization_id: Uuid,
    pub plan: String,
    /// JSON serializado como cadena, no objeto embebido.
    pub features_json: String,
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub valid_until: OffsetDateTime,
    pub grant_json: String,
    pub signature_b64: String,
}

impl From<IssuedLicense> for LicenseRecordResponse {
    fn from(value: IssuedLicense) -> Self {
        Self {
            grant_id: value.grant_id.as_uuid(),
            installation_id: value.installation_id.as_uuid(),
            organization_id: value.organization_id.as_uuid(),
            plan: value.plan,
            features_json: value.features_json,
            issued_at: value.issued_at,
            valid_until: value.valid_until,
            grant_json: value.grant_json,
            signature_b64: value.signature_b64,
        }
    }
}
