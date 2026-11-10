-- C6.1: índices de lectura para Audit y Security Center.
-- Append-only: no crea tablas, columnas, datos ni triggers de retención.

CREATE INDEX staff_audit_event_occurred_idx
    ON staff_audit_event (occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_action_occurred_idx
    ON staff_audit_event (action, occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_org_occurred_idx
    ON staff_audit_event (organization_id, occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_product_occurred_idx
    ON staff_audit_event (product_id, occurred_at DESC, id DESC);

CREATE INDEX commercial_security_event_created_idx
    ON commercial_security_event (created_at DESC, id DESC);
CREATE INDEX commercial_security_event_kind_created_idx
    ON commercial_security_event (kind, created_at DESC, id DESC);
CREATE INDEX commercial_security_event_installation_created_idx
    ON commercial_security_event (installation_id, created_at DESC, id DESC);
CREATE INDEX commercial_security_event_account_created_idx
    ON commercial_security_event (account_id, created_at DESC, id DESC);

CREATE INDEX staff_auth_event_occurred_idx
    ON staff_auth_event (occurred_at DESC, id DESC);
