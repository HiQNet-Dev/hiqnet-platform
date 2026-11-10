//! Adaptador PostgreSQL de planes, suscripciones y entitlements (B6).
//!
//! Organizado por responsabilidad técnica: mappers, SQL, helpers, repositorio y
//! portal. La transacción y los bloqueos `FOR UPDATE` viven en `repository.rs`.

use async_trait::async_trait;
use platform_application::commercial_domain::{
    CommercialError, CommercialRepository, CommercialResult, CreateSubscriptionCommand, CursorKey,
    Entitlements, NewPlan, Plan, PlanFilters, PlanPatch, PlanRef, PlanStatus, PortalProduct,
    PortalProductRef, PortalSubscription, RenewSubscriptionCommand, Subscription,
    SubscriptionAction, SubscriptionDetail, SubscriptionEvent, SubscriptionFilters,
    SubscriptionStatus,
};
use platform_core::ids::{CommercialAccountId, OrganizationId, ProductId, StaffAccountId};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use std::collections::BTreeMap;
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

mod helpers;
mod mappers;
mod portal;
mod repository;
mod sql;

use helpers::*;
use mappers::*;
use portal::*;
use sql::*;

pub struct PgCommercialDomainRepository {
    pool: PgPool,
}

impl PgCommercialDomainRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
