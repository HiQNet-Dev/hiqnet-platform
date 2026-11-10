//! Lecturas read-only de Audit y Security Center (C6.1).
//!
//! Tres datasets separados, sin unión física ni pseudo-evento genérico:
//! `staff_audit_event`, `commercial_security_event` y `staff_auth_event`.
//! La metadata JSONB nunca se envía cruda: se filtra por whitelist.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use platform_core::{
    ids::{CommercialAccountId, InstallationId, OrganizationId, ProductId, StaffAccountId},
    staff::{Capability, StaffRole},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuditSecurityError {
    #[error("recurso no encontrado")]
    NotFound,
    #[error("capability insuficiente para la operación")]
    Forbidden,
    #[error("parámetros de consulta inválidos")]
    InvalidInput,
    #[error("no fue posible completar la operación")]
    Storage,
}

pub type AuditSecurityResult<T> = Result<T, AuditSecurityError>;

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 100;
pub const MAX_CURSOR_LEN: usize = 512;
pub const MAX_ACTION_LEN: usize = 100;
pub const MAX_RESOURCE_TYPE_LEN: usize = 100;

macro_rules! read_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl FromStr for $name {
            type Err = AuditSecurityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value { $($text => Ok(Self::$variant),)+ _ => Err(AuditSecurityError::InvalidInput) }
            }
        }
    };
}

read_enum!(AuditResult {
    Success => "success",
    Denied => "denied",
    Failure => "failure",
});

read_enum!(SecurityKind {
    EnrollmentConflict => "enrollment_conflict",
    DuplicateKey => "duplicate_key",
    TenantChangeAttempt => "tenant_change_attempt",
    ReplayRejected => "replay_rejected",
    RehostPerformed => "rehost_performed",
    IdentityRevoked => "identity_revoked",
    AuthorizationRevokedDuringProof => "authorization_revoked_during_proof",
    IdempotencyConflict => "idempotency_conflict",
    CloneSuspected => "clone_suspected",
});

read_enum!(StaffAuthAction {
    LoginSuccess => "login_success",
    LoginFailure => "login_failure",
    Logout => "logout",
    SessionRevoked => "session_revoked",
    AccountLocked => "account_locked",
    PasswordChanged => "password_changed",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataset {
    Audit,
    Security,
    StaffAuth,
}

impl Dataset {
    fn as_str(self) -> &'static str {
        match self {
            Self::Audit => "audit",
            Self::Security => "security",
            Self::StaffAuth => "staff_auth",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilters {
    pub staff_account_id: Option<StaffAccountId>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub organization_id: Option<OrganizationId>,
    pub product_id: Option<ProductId>,
    pub result: Option<AuditResult>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
}

impl AuditFilters {
    fn validate(&self) -> AuditSecurityResult<()> {
        validate_text(self.action.as_deref(), MAX_ACTION_LEN)?;
        validate_text(self.resource_type.as_deref(), MAX_RESOURCE_TYPE_LEN)?;
        validate_range(self.from, self.to)
    }

    fn binding_hash(&self) -> String {
        let canonical = format!(
            "staff_account_id={};action={};resource_type={};resource_id={};organization_id={};product_id={};result={};from={};to={}",
            opt_id(self.staff_account_id.map(|id| id.as_uuid())),
            opt_str(self.action.as_deref()),
            opt_str(self.resource_type.as_deref()),
            opt_id(self.resource_id),
            opt_id(self.organization_id.map(|id| id.as_uuid())),
            opt_id(self.product_id.map(|id| id.as_uuid())),
            opt_str(self.result.map(AuditResult::as_str)),
            opt_time(self.from),
            opt_time(self.to),
        );
        filter_hash(canonical.as_bytes())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityFilters {
    pub kind: Option<SecurityKind>,
    pub organization_id: Option<OrganizationId>,
    pub installation_id: Option<InstallationId>,
    pub account_id: Option<CommercialAccountId>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
}

impl SecurityFilters {
    fn validate(&self) -> AuditSecurityResult<()> {
        validate_range(self.from, self.to)
    }

    fn binding_hash(&self) -> String {
        let canonical = format!(
            "kind={};organization_id={};installation_id={};account_id={};from={};to={}",
            opt_str(self.kind.map(SecurityKind::as_str)),
            opt_id(self.organization_id.map(|id| id.as_uuid())),
            opt_id(self.installation_id.map(|id| id.as_uuid())),
            opt_id(self.account_id.map(|id| id.as_uuid())),
            opt_time(self.from),
            opt_time(self.to),
        );
        filter_hash(canonical.as_bytes())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaffAuthFilters {
    pub account_id: Option<StaffAccountId>,
    pub action: Option<StaffAuthAction>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
}

impl StaffAuthFilters {
    fn validate(&self) -> AuditSecurityResult<()> {
        validate_range(self.from, self.to)
    }

    fn binding_hash(&self) -> String {
        let canonical = format!(
            "account_id={};action={};from={};to={}",
            opt_id(self.account_id.map(|id| id.as_uuid())),
            opt_str(self.action.map(StaffAuthAction::as_str)),
            opt_time(self.from),
            opt_time(self.to),
        );
        filter_hash(canonical.as_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorKey {
    pub timestamp: OffsetDateTime,
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorPayload {
    v: u8,
    k: String,
    t: i128,
    i: Uuid,
    f: String,
}

impl CursorKey {
    fn encode(self, dataset: Dataset, filters_hash: &str) -> String {
        let payload = CursorPayload {
            v: 1,
            k: dataset.as_str().to_owned(),
            t: self.timestamp.unix_timestamp_nanos(),
            i: self.id,
            f: filters_hash.to_owned(),
        };
        let json = serde_json::to_vec(&payload).expect("cursor serializable");
        URL_SAFE_NO_PAD.encode(json)
    }

    fn decode(value: &str, dataset: Dataset, filters_hash: &str) -> AuditSecurityResult<Self> {
        if value.is_empty() || value.len() > MAX_CURSOR_LEN {
            return Err(AuditSecurityError::InvalidInput);
        }
        let decoded = URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| AuditSecurityError::InvalidInput)?;
        let payload: CursorPayload =
            serde_json::from_slice(&decoded).map_err(|_| AuditSecurityError::InvalidInput)?;
        if payload.v != 1 || payload.k != dataset.as_str() || payload.f != filters_hash {
            return Err(AuditSecurityError::InvalidInput);
        }
        let timestamp = OffsetDateTime::from_unix_timestamp_nanos(payload.t)
            .map_err(|_| AuditSecurityError::InvalidInput)?;
        Ok(Self {
            timestamp,
            id: payload.i,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffActorRef {
    pub account_id: StaffAccountId,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffActorDetail {
    pub account_id: StaffAccountId,
    pub display_name: String,
    pub email: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationRef {
    pub id: OrganizationId,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRef {
    pub id: ProductId,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialAccountRef {
    pub id: CommercialAccountId,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialAccountDetail {
    pub id: CommercialAccountId,
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRef {
    pub id: InstallationId,
    pub device_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEventSummary {
    pub id: Uuid,
    pub occurred_at: OffsetDateTime,
    pub actor: Option<StaffActorRef>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub resource_label: Option<String>,
    pub organization: Option<OrganizationRef>,
    pub product: Option<ProductRef>,
    pub result: AuditResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEventDetail {
    pub summary: AuditEventSummary,
    pub actor: Option<StaffActorDetail>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityEventSummary {
    pub id: Uuid,
    pub occurred_at: OffsetDateTime,
    pub kind: SecurityKind,
    pub organization: Option<OrganizationRef>,
    pub installation: Option<InstallationRef>,
    pub account: Option<CommercialAccountRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecurityEventDetail {
    pub summary: SecurityEventSummary,
    pub account: Option<CommercialAccountDetail>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffAuthEventSummary {
    pub id: Uuid,
    pub occurred_at: OffsetDateTime,
    pub action: StaffAuthAction,
    pub account: Option<StaffActorRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaffAuthEventDetail {
    pub summary: StaffAuthEventSummary,
    pub account: Option<StaffActorDetail>,
    pub metadata: Value,
}

#[async_trait]
pub trait AuditSecurityRepository: Send + Sync {
    async fn list_audit(
        &self,
        filters: &AuditFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<AuditEventSummary>>;
    async fn get_audit(&self, id: Uuid) -> AuditSecurityResult<Option<AuditEventDetail>>;
    async fn list_security(
        &self,
        filters: &SecurityFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<SecurityEventSummary>>;
    async fn get_security(&self, id: Uuid) -> AuditSecurityResult<Option<SecurityEventDetail>>;
    async fn list_staff_auth(
        &self,
        filters: &StaffAuthFilters,
        limit: u32,
        after: Option<CursorKey>,
    ) -> AuditSecurityResult<Vec<StaffAuthEventSummary>>;
    async fn get_staff_auth(&self, id: Uuid) -> AuditSecurityResult<Option<StaffAuthEventDetail>>;
}

#[derive(Debug, Clone, Copy)]
pub struct AuditSecurityActor {
    account_id: StaffAccountId,
    capabilities: &'static [Capability],
}

impl AuditSecurityActor {
    pub fn new(account_id: StaffAccountId, role: StaffRole) -> Self {
        Self {
            account_id,
            capabilities: role.capabilities(),
        }
    }

    pub fn from_capabilities(
        account_id: StaffAccountId,
        capabilities: &'static [Capability],
    ) -> Self {
        Self {
            account_id,
            capabilities,
        }
    }

    pub fn account_id(self) -> StaffAccountId {
        self.account_id
    }

    fn require(&self, capability: Capability) -> AuditSecurityResult<()> {
        if self.capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(AuditSecurityError::Forbidden)
        }
    }
}

pub struct AuditSecurityService<R: AuditSecurityRepository> {
    repository: R,
}

impl<R: AuditSecurityRepository> AuditSecurityService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn list_audit(
        &self,
        actor: &AuditSecurityActor,
        filters: AuditFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> AuditSecurityResult<Page<AuditEventSummary>> {
        actor.require(Capability::AuditRead)?;
        filters.validate()?;
        let (limit, after) = page_params(
            limit,
            cursor.as_deref(),
            Dataset::Audit,
            &filters.binding_hash(),
        )?;
        let mut items = self
            .repository
            .list_audit(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(
            &mut items,
            limit,
            Dataset::Audit,
            |item| (item.occurred_at, item.id),
            &filters.binding_hash(),
        );
        Ok(Page { items, next_cursor })
    }

    pub async fn get_audit(
        &self,
        actor: &AuditSecurityActor,
        id: Uuid,
    ) -> AuditSecurityResult<AuditEventDetail> {
        actor.require(Capability::AuditRead)?;
        let mut detail = self
            .repository
            .get_audit(id)
            .await?
            .ok_or(AuditSecurityError::NotFound)?;
        detail.metadata = sanitize_audit_metadata(&detail.summary.action, detail.metadata);
        Ok(detail)
    }

    pub async fn list_security(
        &self,
        actor: &AuditSecurityActor,
        filters: SecurityFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> AuditSecurityResult<Page<SecurityEventSummary>> {
        actor.require(Capability::SecurityRead)?;
        filters.validate()?;
        let (limit, after) = page_params(
            limit,
            cursor.as_deref(),
            Dataset::Security,
            &filters.binding_hash(),
        )?;
        let mut items = self
            .repository
            .list_security(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(
            &mut items,
            limit,
            Dataset::Security,
            |item| (item.occurred_at, item.id),
            &filters.binding_hash(),
        );
        Ok(Page { items, next_cursor })
    }

    pub async fn get_security(
        &self,
        actor: &AuditSecurityActor,
        id: Uuid,
    ) -> AuditSecurityResult<SecurityEventDetail> {
        actor.require(Capability::SecurityRead)?;
        let mut detail = self
            .repository
            .get_security(id)
            .await?
            .ok_or(AuditSecurityError::NotFound)?;
        detail.metadata = sanitize_security_metadata(detail.summary.kind, detail.metadata);
        Ok(detail)
    }

    pub async fn list_staff_auth(
        &self,
        actor: &AuditSecurityActor,
        filters: StaffAuthFilters,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> AuditSecurityResult<Page<StaffAuthEventSummary>> {
        actor.require(Capability::SecurityRead)?;
        filters.validate()?;
        let (limit, after) = page_params(
            limit,
            cursor.as_deref(),
            Dataset::StaffAuth,
            &filters.binding_hash(),
        )?;
        let mut items = self
            .repository
            .list_staff_auth(&filters, limit + 1, after)
            .await?;
        let next_cursor = take_page_cursor(
            &mut items,
            limit,
            Dataset::StaffAuth,
            |item| (item.occurred_at, item.id),
            &filters.binding_hash(),
        );
        Ok(Page { items, next_cursor })
    }

    pub async fn get_staff_auth(
        &self,
        actor: &AuditSecurityActor,
        id: Uuid,
    ) -> AuditSecurityResult<StaffAuthEventDetail> {
        actor.require(Capability::SecurityRead)?;
        let mut detail = self
            .repository
            .get_staff_auth(id)
            .await?
            .ok_or(AuditSecurityError::NotFound)?;
        detail.metadata = sanitize_staff_auth_metadata(detail.summary.action, detail.metadata);
        Ok(detail)
    }
}

fn page_params(
    limit: Option<u32>,
    cursor: Option<&str>,
    dataset: Dataset,
    filters_hash: &str,
) -> AuditSecurityResult<(u32, Option<CursorKey>)> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(AuditSecurityError::InvalidInput);
    }
    let after = cursor
        .map(|value| CursorKey::decode(value, dataset, filters_hash))
        .transpose()?;
    Ok((limit, after))
}

fn take_page_cursor<T>(
    items: &mut Vec<T>,
    limit: u32,
    dataset: Dataset,
    key: impl Fn(&T) -> (OffsetDateTime, Uuid),
    filters_hash: &str,
) -> Option<String> {
    if items.len() as u32 > limit {
        items.pop();
        items.last().map(|item| {
            let (timestamp, id) = key(item);
            CursorKey { timestamp, id }.encode(dataset, filters_hash)
        })
    } else {
        None
    }
}

fn validate_text(value: Option<&str>, max_len: usize) -> AuditSecurityResult<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.chars().count() > max_len {
            return Err(AuditSecurityError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_range(
    from: Option<OffsetDateTime>,
    to: Option<OffsetDateTime>,
) -> AuditSecurityResult<()> {
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err(AuditSecurityError::InvalidInput);
        }
    }
    Ok(())
}

fn filter_hash(canonical: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(canonical))
}

fn opt_str(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn opt_id(value: Option<Uuid>) -> String {
    value
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn opt_time(value: Option<OffsetDateTime>) -> String {
    value
        .map(|time| time.unix_timestamp_nanos().to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn pick(metadata: Value, keys: &[&str]) -> Value {
    let Value::Object(object) = metadata else {
        return Value::Object(Map::new());
    };
    let mut result = Map::new();
    for key in keys {
        if let Some(value) = object.get(*key) {
            result.insert((*key).to_owned(), value.clone());
        }
    }
    Value::Object(result)
}

pub fn sanitize_audit_metadata(action: &str, metadata: Value) -> Value {
    match action {
        "installation_suspended" | "installation_reactivated" => {
            pick(metadata, &["before", "after"])
        }
        "installation_rehost_authorized" => {
            pick(metadata, &["enrollment_request_id", "reason", "expires_at"])
        }
        "installation_binding_revoked" => pick(metadata, &["reason"]),
        "installation_rehost_completed" => {
            pick(metadata, &["enrollment_request_id", "identity_id"])
        }
        "installation_identity_retired" => Value::Object(Map::new()),
        _ => Value::Object(Map::new()),
    }
}

pub fn sanitize_security_metadata(kind: SecurityKind, metadata: Value) -> Value {
    match kind {
        SecurityKind::RehostPerformed => pick(metadata, &["enrollment_request_id"]),
        SecurityKind::IdentityRevoked => pick(metadata, &["reason"]),
        _ => Value::Object(Map::new()),
    }
}

pub fn sanitize_staff_auth_metadata(action: StaffAuthAction, metadata: Value) -> Value {
    match action {
        StaffAuthAction::LoginFailure => {
            let sanitized = pick(metadata, &["reason"]);
            let reason = sanitized
                .get("reason")
                .and_then(Value::as_str)
                .filter(|reason| {
                    matches!(
                        *reason,
                        "unknown"
                            | "no_credential"
                            | "locked"
                            | "bad_password"
                            | "credential_changed"
                            | "account_unavailable"
                            | "unexpected"
                    )
                });
            match reason {
                Some(reason) => serde_json::json!({ "reason": reason }),
                None => Value::Object(Map::new()),
            }
        }
        _ => Value::Object(Map::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: StaffRole) -> AuditSecurityActor {
        AuditSecurityActor::new(StaffAccountId::new(), role)
    }

    #[test]
    fn cursor_binds_dataset_and_filters() {
        let filters = AuditFilters::default();
        let hash = filters.binding_hash();
        let key = CursorKey {
            timestamp: OffsetDateTime::now_utc(),
            id: Uuid::new_v4(),
        };
        let encoded = key.encode(Dataset::Audit, &hash);
        assert_eq!(
            CursorKey::decode(&encoded, Dataset::Audit, &hash).unwrap(),
            key
        );
        assert_eq!(
            CursorKey::decode(&encoded, Dataset::Security, &hash),
            Err(AuditSecurityError::InvalidInput)
        );
        let other = AuditFilters {
            action: Some("product_created".into()),
            ..AuditFilters::default()
        };
        assert_eq!(
            CursorKey::decode(&encoded, Dataset::Audit, &other.binding_hash()),
            Err(AuditSecurityError::InvalidInput)
        );
        assert_eq!(
            CursorKey::decode("not-base64", Dataset::Audit, &hash),
            Err(AuditSecurityError::InvalidInput)
        );
    }

    #[test]
    fn limit_and_range_are_strict() {
        assert_eq!(
            page_params(Some(0), None, Dataset::Audit, "f"),
            Err(AuditSecurityError::InvalidInput)
        );
        assert_eq!(
            page_params(Some(101), None, Dataset::Audit, "f"),
            Err(AuditSecurityError::InvalidInput)
        );
        let now = OffsetDateTime::now_utc();
        let filters = AuditFilters {
            from: Some(now),
            to: Some(now - time::Duration::seconds(1)),
            ..AuditFilters::default()
        };
        assert_eq!(filters.validate(), Err(AuditSecurityError::InvalidInput));
    }

    #[test]
    fn metadata_whitelist_and_secret_drop() {
        let metadata = serde_json::json!({
            "before": "active",
            "after": "suspended",
            "token_hash": "secret",
            "password_hash": "secret"
        });
        assert_eq!(
            sanitize_audit_metadata("installation_suspended", metadata),
            serde_json::json!({ "before": "active", "after": "suspended" })
        );
        assert_eq!(
            sanitize_audit_metadata(
                "installation_identity_retired",
                serde_json::json!({
                    "generation": "active"
                })
            ),
            serde_json::json!({})
        );
        assert_eq!(
            sanitize_security_metadata(
                SecurityKind::RehostPerformed,
                serde_json::json!({ "enrollment_request_id": "id", "nonce": "secret" })
            ),
            serde_json::json!({ "enrollment_request_id": "id" })
        );
        assert_eq!(
            sanitize_staff_auth_metadata(
                StaffAuthAction::LoginFailure,
                serde_json::json!({ "reason": "bad_password", "ip_hash": "secret" })
            ),
            serde_json::json!({ "reason": "bad_password" })
        );
        assert_eq!(
            sanitize_staff_auth_metadata(
                StaffAuthAction::LoginFailure,
                serde_json::json!({ "reason": "raw-db-detail" })
            ),
            serde_json::json!({})
        );
    }

    #[test]
    fn capabilities_match_c6_contract() {
        for role in [
            StaffRole::SuperAdmin,
            StaffRole::PlatformAdmin,
            StaffRole::Operator,
            StaffRole::Support,
            StaffRole::SecurityAuditor,
            StaffRole::ReadOnly,
        ] {
            assert!(actor(role).require(Capability::AuditRead).is_ok());
            assert!(actor(role).require(Capability::SecurityRead).is_ok());
        }
        assert_eq!(
            AuditSecurityActor::from_capabilities(StaffAccountId::new(), &[])
                .require(Capability::SecurityRead),
            Err(AuditSecurityError::Forbidden)
        );
    }
}
