//! Validaciones e idempotencia de planes y suscripciones.

use super::*;

pub(super) fn validate_plan(plan: &NewPlan) -> CommercialResult<()> {
    validate_key(&plan.key)?;
    validate_name(&plan.name)?;
    if plan.description.chars().count() > 2000 {
        return Err(CommercialError::InvalidInput);
    }
    validate_features(&plan.features)?;
    validate_limits(&plan.limits)?;
    Ok(())
}

pub(super) fn validate_name(value: &str) -> CommercialResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_key(value: &str) -> CommercialResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_features(features: &BTreeMap<String, bool>) -> CommercialResult<()> {
    for key in features.keys() {
        validate_key(key)?;
    }
    Ok(())
}

pub(super) fn validate_limits(limits: &BTreeMap<String, i64>) -> CommercialResult<()> {
    for (key, value) in limits {
        validate_key(key)?;
        if *value < 0 {
            return Err(CommercialError::InvalidInput);
        }
    }
    Ok(())
}

pub(super) fn validate_idempotency_key(value: &str) -> CommercialResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_IDEMPOTENCY_KEY_LEN {
        return Err(CommercialError::InvalidInput);
    }
    Ok(())
}

pub(super) fn hash_secret(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

pub(super) fn create_request_hash(request: &CreateSubscription) -> Vec<u8> {
    let canonical = format!(
        "create:{}:{}:{}:{}:{}",
        request.organization_id.as_uuid(),
        request.product_id.as_uuid(),
        request.plan_id,
        request.starts_at.unix_timestamp_nanos(),
        request.ends_at.unix_timestamp_nanos()
    );
    hash_secret(canonical.as_bytes()).to_vec()
}

pub(super) fn renew_request_hash(request: &RenewSubscription) -> Vec<u8> {
    let canonical = format!(
        "renew:{}:{}",
        request
            .plan_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".into()),
        request.ends_at.unix_timestamp_nanos()
    );
    hash_secret(canonical.as_bytes()).to_vec()
}
