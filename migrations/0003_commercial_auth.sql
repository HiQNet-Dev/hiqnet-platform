-- Credenciales y sesiones comerciales. No modifica 0001 ni 0002.
CREATE TABLE commercial_credential (
    account_id UUID PRIMARY KEY REFERENCES commercial_account (id) ON DELETE RESTRICT,
    password_hash TEXT NOT NULL,
    password_changed_at TIMESTAMPTZ NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT credential_lock_order CHECK (locked_until IS NULL OR locked_until >= created_at)
);

CREATE TABLE commercial_session (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES commercial_account (id) ON DELETE RESTRICT,
    token_hash BYTEA NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ NOT NULL,
    user_agent_hash BYTEA,
    ip_hash BYTEA,
    CONSTRAINT session_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT session_revocation_order CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE INDEX commercial_session_account_active_idx
    ON commercial_session (account_id, expires_at DESC) WHERE revoked_at IS NULL;
CREATE INDEX commercial_session_expiry_idx
    ON commercial_session (expires_at) WHERE revoked_at IS NULL;

CREATE TABLE commercial_auth_event (
    id UUID PRIMARY KEY,
    account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK (action IN ('login_success', 'login_failure', 'logout', 'session_revoked', 'account_locked', 'password_changed')),
    occurred_at TIMESTAMPTZ NOT NULL,
    ip_hash BYTEA,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX commercial_auth_event_account_time_idx
    ON commercial_auth_event (account_id, occurred_at DESC);
CREATE INDEX commercial_auth_event_action_time_idx
    ON commercial_auth_event (action, occurred_at DESC);

CREATE FUNCTION reject_commercial_auth_event_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'commercial_auth_event es inmutable';
END;
$$;

CREATE TRIGGER commercial_auth_event_append_only
BEFORE UPDATE OR DELETE ON commercial_auth_event
FOR EACH ROW EXECUTE FUNCTION reject_commercial_auth_event_change();
