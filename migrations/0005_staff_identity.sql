-- Identidad, credenciales, sesiones y auditoría del personal interno HiQNet
-- (Control Center). No modifica 0001–0004 y no representa clientes ni usuarios
-- locales del POS.

CREATE TABLE staff_account (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT staff_account_email_canonical CHECK (
        email = lower(btrim(email))
        AND length(email) BETWEEN 3 AND 320
        AND email LIKE '%@%'
    ),
    CONSTRAINT staff_account_name_nonempty CHECK (length(btrim(display_name)) BETWEEN 1 AND 200),
    CONSTRAINT staff_account_update_order CHECK (updated_at >= created_at)
);

CREATE TABLE staff_credential (
    account_id UUID PRIMARY KEY REFERENCES staff_account (id) ON DELETE RESTRICT,
    password_hash TEXT NOT NULL,
    password_changed_at TIMESTAMPTZ NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT staff_credential_lock_order CHECK (locked_until IS NULL OR locked_until >= created_at)
);

CREATE TABLE staff_session (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES staff_account (id) ON DELETE RESTRICT,
    token_hash BYTEA NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ NOT NULL,
    ip_hash BYTEA,
    user_agent_hash BYTEA,
    CONSTRAINT staff_session_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT staff_session_revocation_order CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE INDEX staff_session_account_active_idx
    ON staff_session (account_id, expires_at DESC) WHERE revoked_at IS NULL;
CREATE INDEX staff_session_expiry_idx
    ON staff_session (expires_at) WHERE revoked_at IS NULL;

-- Un único rol activo por cuenta en C1. El modelo admite historial y varias
-- asignaciones si una fase posterior lo requiere.
CREATE TABLE staff_role_assignment (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES staff_account (id) ON DELETE RESTRICT,
    role TEXT NOT NULL CHECK (role IN (
        'super_admin', 'platform_admin', 'operator', 'support', 'security_auditor', 'read_only'
    )),
    granted_by UUID REFERENCES staff_account (id) ON DELETE RESTRICT,
    granted_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT staff_role_revocation_order CHECK (revoked_at IS NULL OR revoked_at >= granted_at)
);

CREATE UNIQUE INDEX staff_role_assignment_one_active
    ON staff_role_assignment (account_id) WHERE revoked_at IS NULL;
CREATE INDEX staff_role_assignment_account_idx
    ON staff_role_assignment (account_id, granted_at DESC);

CREATE TABLE staff_auth_event (
    id UUID PRIMARY KEY,
    account_id UUID REFERENCES staff_account (id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK (action IN (
        'login_success', 'login_failure', 'logout', 'session_revoked',
        'account_locked', 'password_changed'
    )),
    occurred_at TIMESTAMPTZ NOT NULL,
    ip_hash BYTEA,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX staff_auth_event_account_time_idx
    ON staff_auth_event (account_id, occurred_at DESC);
CREATE INDEX staff_auth_event_action_time_idx
    ON staff_auth_event (action, occurred_at DESC);

CREATE FUNCTION reject_staff_auth_event_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'staff_auth_event es inmutable';
END;
$$;

CREATE TRIGGER staff_auth_event_append_only
BEFORE UPDATE OR DELETE ON staff_auth_event
FOR EACH ROW EXECUTE FUNCTION reject_staff_auth_event_change();

-- Auditoría de mutaciones administrativas. `product_id` queda sin FK hasta que
-- exista la tabla de productos (C2); se mantiene para trazabilidad futura.
CREATE TABLE staff_audit_event (
    id UUID PRIMARY KEY,
    staff_account_id UUID REFERENCES staff_account (id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id UUID,
    organization_id UUID REFERENCES organization (id) ON DELETE RESTRICT,
    product_id UUID,
    result TEXT NOT NULL CHECK (result IN ('success', 'denied', 'failure')),
    occurred_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT staff_audit_resource_nonempty CHECK (length(btrim(resource_type)) BETWEEN 1 AND 100),
    CONSTRAINT staff_audit_action_nonempty CHECK (length(btrim(action)) BETWEEN 1 AND 100)
);

CREATE INDEX staff_audit_event_staff_time_idx
    ON staff_audit_event (staff_account_id, occurred_at DESC);
CREATE INDEX staff_audit_event_resource_idx
    ON staff_audit_event (resource_type, resource_id);

CREATE FUNCTION reject_staff_audit_event_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'staff_audit_event es inmutable';
END;
$$;

CREATE TRIGGER staff_audit_event_append_only
BEFORE UPDATE OR DELETE ON staff_audit_event
FOR EACH ROW EXECUTE FUNCTION reject_staff_audit_event_change();
