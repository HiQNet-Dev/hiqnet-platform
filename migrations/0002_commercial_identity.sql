-- Identidad comercial sin credenciales de sesión: P02.2 incorporará autenticación.
CREATE TABLE commercial_account (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT commercial_account_email_canonical CHECK (email = lower(btrim(email)) AND length(email) BETWEEN 3 AND 320 AND email LIKE '%@%'),
    CONSTRAINT commercial_account_name_nonempty CHECK (length(btrim(display_name)) BETWEEN 1 AND 200)
);

CREATE TABLE organization (
    id UUID PRIMARY KEY,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'legacy_unclaimed', 'archived')),
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT organization_name_nonempty CHECK (length(btrim(display_name)) BETWEEN 1 AND 200)
);

-- Las instalaciones P00/P01 conservan sus UUID; sus organizaciones se incorporan
-- sin dueño ni membresías. Por tanto, nadie hereda acceso automáticamente.
INSERT INTO organization (id, display_name, status, created_at)
SELECT organization_id, 'Organización previa sin reclamar', 'legacy_unclaimed', min(registered_at)
FROM installation
GROUP BY organization_id;

CREATE TABLE organization_membership (
    organization_id UUID NOT NULL REFERENCES organization (id) ON DELETE RESTRICT,
    account_id UUID NOT NULL REFERENCES commercial_account (id) ON DELETE RESTRICT,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    role_version SMALLINT NOT NULL DEFAULT 1 CHECK (role_version = 1),
    joined_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (organization_id, account_id),
    CONSTRAINT membership_revocation_order CHECK (revoked_at IS NULL OR revoked_at >= joined_at)
);
CREATE INDEX organization_membership_account_active_idx
    ON organization_membership (account_id, organization_id) WHERE revoked_at IS NULL;

CREATE TABLE commercial_audit_event (
    id UUID PRIMARY KEY,
    organization_id UUID REFERENCES organization (id) ON DELETE RESTRICT,
    actor_account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    subject_account_id UUID REFERENCES commercial_account (id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK (action IN ('account_created', 'organization_created', 'membership_added', 'membership_role_changed', 'membership_revoked', 'legacy_import')),
    old_role TEXT CHECK (old_role IN ('owner', 'admin', 'member')),
    new_role TEXT CHECK (new_role IN ('owner', 'admin', 'member')),
    occurred_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT audit_actor_required CHECK (actor_account_id IS NOT NULL OR action IN ('legacy_import', 'account_created')),
    CONSTRAINT audit_org_required CHECK (organization_id IS NOT NULL OR action = 'account_created'),
    CONSTRAINT audit_role_transition CHECK (
        (action = 'membership_added' AND old_role IS NULL AND new_role IS NOT NULL)
        OR (action = 'membership_role_changed' AND old_role IS NOT NULL AND new_role IS NOT NULL)
        OR (action = 'membership_revoked' AND old_role IS NOT NULL AND new_role IS NULL)
        OR (action NOT IN ('membership_added', 'membership_role_changed', 'membership_revoked') AND old_role IS NULL AND new_role IS NULL)
    )
);
CREATE INDEX commercial_audit_event_org_time_idx
    ON commercial_audit_event (organization_id, occurred_at DESC);
CREATE INDEX commercial_audit_event_subject_time_idx
    ON commercial_audit_event (subject_account_id, occurred_at DESC);

INSERT INTO commercial_audit_event (id, organization_id, action, occurred_at)
SELECT gen_random_uuid(), id, 'legacy_import', created_at
FROM organization WHERE status = 'legacy_unclaimed';

-- La auditoría no se sobrescribe ni se borra mediante operaciones normales.
CREATE FUNCTION reject_commercial_audit_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'commercial_audit_event es inmutable';
END;
$$;
CREATE TRIGGER commercial_audit_append_only
BEFORE UPDATE OR DELETE ON commercial_audit_event
FOR EACH ROW EXECUTE FUNCTION reject_commercial_audit_change();

ALTER TABLE installation ADD CONSTRAINT installation_organization_id_fkey
    FOREIGN KEY (organization_id) REFERENCES organization (id) ON DELETE RESTRICT;
ALTER TABLE installation ADD CONSTRAINT installation_id_organization_unique
    UNIQUE (id, organization_id);

-- No eliminar autorizaciones al borrar instalaciones; tampoco permitir que
-- una licencia apunte a una organización distinta de la instalación firmada.
ALTER TABLE issued_license DROP CONSTRAINT issued_license_installation_id_fkey;
ALTER TABLE issued_license ADD CONSTRAINT issued_license_installation_org_fkey
    FOREIGN KEY (installation_id, organization_id)
    REFERENCES installation (id, organization_id) ON DELETE RESTRICT;
