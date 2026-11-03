-- Esquema inicial de HiQNet Platform (PostgreSQL).
-- Solo tablas de control plane: instalaciones y autorizaciones emitidas.
-- No hay tablas de venta ni catálogo (eso es responsabilidad del POS local).

CREATE TABLE IF NOT EXISTS installation (
    id              UUID PRIMARY KEY,
    organization_id UUID NOT NULL,
    device_label    TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('active', 'suspended', 'unregistered')),
    registered_at   TIMESTAMPTZ NOT NULL,
    last_seen_at    TIMESTAMPTZ,
    trial_started_at TIMESTAMPTZ,
    trial_ends_at   TIMESTAMPTZ,
    CONSTRAINT installation_org_device_unique UNIQUE (organization_id, device_label)
);

CREATE INDEX IF NOT EXISTS idx_installation_org ON installation (organization_id);

CREATE TABLE IF NOT EXISTS issued_license (
    grant_id        UUID PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES installation (id) ON DELETE CASCADE,
    organization_id UUID NOT NULL,
    plan            TEXT NOT NULL CHECK (plan IN ('trial', 'local', 'pro', 'enterprise')),
    features_json   JSONB NOT NULL,
    issued_at       TIMESTAMPTZ NOT NULL,
    valid_until     TIMESTAMPTZ NOT NULL,
    grant_json      TEXT NOT NULL,
    signature_b64   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_issued_license_installation
    ON issued_license (installation_id, issued_at DESC);
