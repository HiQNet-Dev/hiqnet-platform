//! Pruebas del dominio comercial.

use super::pagination::*;
use super::validation::*;
use super::*;

fn actor(role: StaffRole) -> CommercialActor {
    CommercialActor::new(StaffAccountId::new(), role)
}

#[test]
fn capabilities_follow_beta_matrix() {
    for role in [
        StaffRole::SuperAdmin,
        StaffRole::PlatformAdmin,
        StaffRole::Operator,
        StaffRole::Support,
        StaffRole::SecurityAuditor,
        StaffRole::ReadOnly,
    ] {
        assert!(actor(role).require(Capability::ProductsRead).is_ok());
        assert!(actor(role).require(Capability::SubscriptionsRead).is_ok());
    }
    for role in [
        StaffRole::Operator,
        StaffRole::Support,
        StaffRole::SecurityAuditor,
        StaffRole::ReadOnly,
    ] {
        assert_eq!(
            actor(role).require(Capability::ProductsManage),
            Err(CommercialError::Forbidden)
        );
        assert_eq!(
            actor(role).require(Capability::SubscriptionsManage),
            Err(CommercialError::Forbidden)
        );
    }
    assert!(
        actor(StaffRole::PlatformAdmin)
            .require(Capability::SubscriptionsManage)
            .is_ok()
    );
}

#[test]
fn plan_status_transitions_are_closed() {
    assert!(PlanStatus::Draft.can_transition_to(PlanStatus::Active));
    assert!(!PlanStatus::Retired.can_transition_to(PlanStatus::Active));
    assert_eq!(parse_plan_status("active"), Ok(PlanStatus::Active));
    assert_eq!(
        parse_plan_status("deleted"),
        Err(CommercialError::InvalidInput)
    );
}

#[test]
fn cursors_are_dataset_bound() {
    let timestamp = OffsetDateTime::now_utc();
    let id = Uuid::new_v4();
    let cursor = encode_cursor("plan", timestamp, id);
    assert_eq!(
        decode_cursor(&cursor, "plan").unwrap(),
        CursorKey { timestamp, id }
    );
    assert_eq!(
        decode_cursor(&cursor, "subscription"),
        Err(CommercialError::InvalidInput)
    );
    assert_eq!(
        decode_cursor("garbage", "plan"),
        Err(CommercialError::InvalidInput)
    );
}

#[test]
fn idempotency_and_validation_are_strict() {
    assert_eq!(
        validate_idempotency_key("  "),
        Err(CommercialError::InvalidInput)
    );
    assert!(validate_idempotency_key("key-1").is_ok());
    assert_eq!(validate_key("bad key"), Err(CommercialError::InvalidInput));
    assert!(validate_key("pos-local").is_ok());
    assert_eq!(
        validate_limits(&BTreeMap::from([("seats".into(), -1)])),
        Err(CommercialError::InvalidInput)
    );
}
