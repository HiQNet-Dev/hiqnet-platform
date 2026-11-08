-- Enrolamiento comercial seguro de instalaciones. Incremental respecto a 0001-0003.
ALTER TABLE installation
    ADD COLUMN IF NOT EXISTS identity_status TEXT NOT NULL DEFAULT 'unenrolled'
        CHECK (identity_status IN ('unenrolled', 'active', 'retired')),
    ADD COLUMN IF NOT EXISTS enrolled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS enrolled_by_account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS installation_identity (
    id UUID PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES installation (id) ON DELETE RESTRICT,
    public_key BYTEA NOT NULL,
    fingerprint BYTEA NOT NULL,
    generation INTEGER NOT NULL CHECK (generation >= 1),
    key_version TEXT NOT NULL DEFAULT 'ed25519:v1',
    created_at TIMESTAMPTZ NOT NULL,
    retired_at TIMESTAMPTZ,
    CONSTRAINT installation_identity_key_len CHECK (octet_length(public_key) = 32),
    CONSTRAINT installation_identity_fingerprint_len CHECK (octet_length(fingerprint) = 32),
    CONSTRAINT installation_identity_generation_unique UNIQUE (installation_id, generation)
);

CREATE UNIQUE INDEX IF NOT EXISTS installation_identity_one_active_idx
    ON installation_identity (installation_id) WHERE retired_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS installation_identity_fingerprint_active_idx
    ON installation_identity (fingerprint) WHERE retired_at IS NULL;

CREATE TABLE IF NOT EXISTS enrollment_request (
    id UUID PRIMARY KEY,
    installation_id UUID NOT NULL,
    public_key BYTEA NOT NULL,
    fingerprint BYTEA NOT NULL,
    device_label TEXT NOT NULL CHECK (length(device_label) BETWEEN 1 AND 120),
    app_version TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('windows', 'linux', 'android', 'unknown')),
    device_code_hash BYTEA NOT NULL,
    user_code_hash BYTEA NOT NULL,
    nonce_hash BYTEA NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'completed', 'expired', 'cancelled')),
    organization_id UUID REFERENCES organization (id) ON DELETE RESTRICT,
    approved_by_account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    idempotency_scope TEXT NOT NULL,
    idempotency_key TEXT,
    request_hash BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ,
    consumed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    CONSTRAINT enrollment_request_key_len CHECK (octet_length(public_key) = 32),
    CONSTRAINT enrollment_request_fingerprint_len CHECK (octet_length(fingerprint) = 32),
    CONSTRAINT enrollment_request_device_code_hash_len CHECK (octet_length(device_code_hash) = 32),
    CONSTRAINT enrollment_request_user_code_hash_len CHECK (octet_length(user_code_hash) = 32),
    CONSTRAINT enrollment_request_nonce_hash_len CHECK (octet_length(nonce_hash) = 32),
    CONSTRAINT enrollment_request_status_times CHECK (
        (status = 'pending' AND approved_at IS NULL AND consumed_at IS NULL) OR
        (status = 'approved' AND organization_id IS NOT NULL AND approved_by_account_id IS NOT NULL AND approved_at IS NOT NULL AND consumed_at IS NULL) OR
        (status = 'completed' AND organization_id IS NOT NULL AND approved_by_account_id IS NOT NULL AND consumed_at IS NOT NULL) OR
        (status IN ('expired', 'cancelled') AND consumed_at IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS enrollment_request_status_expires_idx
    ON enrollment_request (status, expires_at);
CREATE UNIQUE INDEX IF NOT EXISTS enrollment_request_device_code_hash_idx
    ON enrollment_request (device_code_hash);
CREATE UNIQUE INDEX IF NOT EXISTS enrollment_request_user_code_active_idx
    ON enrollment_request (user_code_hash)
    WHERE status IN ('pending', 'approved');
CREATE INDEX IF NOT EXISTS enrollment_request_org_created_idx
    ON enrollment_request (organization_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS enrollment_request_idempotency_idx
    ON enrollment_request (idempotency_scope, idempotency_key) WHERE idempotency_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS idempotency_record (
    id UUID PRIMARY KEY,
    scope TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    key TEXT NOT NULL,
    request_hash BYTEA NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'failed')),
    response_status INTEGER,
    response_body JSONB,
    secrets_delivered BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT idempotency_secrets_response_check CHECK (NOT secrets_delivered OR response_body IS NULL)
);

CREATE UNIQUE INDEX IF NOT EXISTS idempotency_record_scope_endpoint_key_idx
    ON idempotency_record (scope, endpoint, key);

CREATE TABLE IF NOT EXISTS installation_binding (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id) ON DELETE RESTRICT,
    installation_id UUID NOT NULL REFERENCES installation (id) ON DELETE RESTRICT,
    enrollment_request_id UUID NOT NULL REFERENCES enrollment_request (id) ON DELETE RESTRICT,
    installation_identity_id UUID NOT NULL REFERENCES installation_identity (id) ON DELETE RESTRICT,
    approved_by_account_id UUID NOT NULL REFERENCES commercial_account (id) ON DELETE RESTRICT,
    bound_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoke_reason TEXT,
    CONSTRAINT installation_binding_org_installation_fk FOREIGN KEY (installation_id, organization_id)
        REFERENCES installation (id, organization_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS installation_binding_one_active_idx
    ON installation_binding (installation_id) WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS commercial_security_event (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN (
        'enrollment_conflict',
        'duplicate_key',
        'tenant_change_attempt',
        'replay_rejected',
        'rehost_performed',
        'identity_revoked',
        'authorization_revoked_during_proof',
        'idempotency_conflict',
        'clone_suspected'
    )),
    organization_id UUID REFERENCES organization (id) ON DELETE RESTRICT,
    account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    installation_id UUID REFERENCES installation (id) ON DELETE RESTRICT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS commercial_security_event_org_created_idx
    ON commercial_security_event (organization_id, created_at DESC);

CREATE FUNCTION reject_commercial_security_event_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'commercial_security_event es inmutable';
END;
$$;

CREATE TRIGGER commercial_security_event_append_only
BEFORE UPDATE OR DELETE ON commercial_security_event
FOR EACH ROW EXECUTE FUNCTION reject_commercial_security_event_change();

-- Auditoría: agregar acciones nuevas sin tocar eventos antiguos.
ALTER TABLE commercial_audit_event
    DROP CONSTRAINT IF EXISTS commercial_audit_event_action_check;
ALTER TABLE commercial_audit_event
    ADD CONSTRAINT commercial_audit_event_action_check CHECK (action IN (
        'account_created',
        'organization_created',
        'membership_added',
        'membership_role_changed',
        'membership_revoked',
        'legacy_import',
        'enrollment_approved',
        'installation_enrolled',
        'installation_identity_registered'
    ));
-- (El trigger append-only existente se conserva; no se reescribe aquí.)
