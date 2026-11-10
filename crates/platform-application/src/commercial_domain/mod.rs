//! Commercial Core mínimo de Beta (ADR-0011).
//!
//! Plan es catálogo por producto; Subscription es el contrato con historial;
//! Entitlement es un read model derivado, nunca `organization_product.entitlements`.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::{
    ids::{OrganizationId, ProductId, StaffAccountId},
    staff::{Capability, StaffRole},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

mod entitlements;
mod model;
mod pagination;
mod plans;
mod ports;
mod subscriptions;
mod validation;

#[cfg(test)]
mod tests;

pub use model::*;
pub use ports::*;

/// Servicio de aplicación del dominio comercial.
pub struct CommercialService<R: CommercialRepository> {
    repository: R,
}

impl<R: CommercialRepository> CommercialService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}
