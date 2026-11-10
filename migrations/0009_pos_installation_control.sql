-- C5: control administrativo de instalaciones POS.
-- Incremental respecto a 0001-0008. No modifica migraciones publicadas.

ALTER TABLE installation
    ADD COLUMN product_id UUID REFERENCES product (id) ON DELETE RESTRICT;

-- Atribución coherente con la asociación organización↔producto.
ALTER TABLE installation
    ADD CONSTRAINT installation_organization_product_fkey
    FOREIGN KEY (organization_id, product_id)
    REFERENCES organization_product (organization_id, product_id)
    ON DELETE RESTRICT;

CREATE INDEX installation_product_idx ON installation (product_id);
-- Soporta el orden keyset (registered_at, id) del listado Control.
CREATE INDEX installation_registered_idx ON installation (registered_at, id);

-- Write-once: NULL -> UUID y mismo UUID permitidos; UUID -> otro UUID prohibido.
CREATE FUNCTION protect_installation_attribution() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.organization_id IS DISTINCT FROM OLD.organization_id THEN
        RAISE EXCEPTION 'installation.organization_id es inmutable';
    END IF;
    IF OLD.product_id IS NOT NULL
       AND NEW.product_id IS DISTINCT FROM OLD.product_id THEN
        RAISE EXCEPTION 'installation.product_id es write-once';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER installation_attribution_write_once
BEFORE UPDATE ON installation
FOR EACH ROW EXECUTE FUNCTION protect_installation_attribution();

-- Sin eliminación física: historial, licencias, identidades y auditoría dependen de ella.
CREATE FUNCTION reject_installation_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'installation no admite eliminación física';
END;
$$;

CREATE TRIGGER installation_no_delete
BEFORE DELETE ON installation
FOR EACH ROW EXECUTE FUNCTION reject_installation_delete();

-- Producto resuelto por Platform durante el enrolment.
ALTER TABLE enrollment_request
    ADD COLUMN product_id UUID REFERENCES product (id) ON DELETE RESTRICT;

CREATE INDEX enrollment_request_product_idx ON enrollment_request (product_id);

CREATE TABLE installation_rehost_authorization (
    id UUID PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES installation (id) ON DELETE RESTRICT,
    enrollment_request_id UUID NOT NULL UNIQUE REFERENCES enrollment_request (id) ON DELETE RESTRICT,
    organization_id UUID NOT NULL REFERENCES organization (id) ON DELETE RESTRICT,
    product_id UUID REFERENCES product (id) ON DELETE RESTRICT,
    authorized_by_staff_account_id UUID NOT NULL REFERENCES staff_account (id) ON DELETE RESTRICT,
    reason TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'consumed', 'expired', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    CONSTRAINT rehost_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT rehost_reason_length CHECK (length(btrim(reason)) BETWEEN 1 AND 500),
    CONSTRAINT rehost_status_times CHECK (
        (status = 'pending' AND consumed_at IS NULL AND cancelled_at IS NULL) OR
        (status = 'consumed' AND consumed_at IS NOT NULL AND cancelled_at IS NULL) OR
        (status = 'expired' AND consumed_at IS NULL AND cancelled_at IS NULL) OR
        (status = 'cancelled' AND consumed_at IS NULL AND cancelled_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX installation_rehost_one_pending_idx
    ON installation_rehost_authorization (installation_id)
    WHERE status = 'pending';
CREATE INDEX installation_rehost_installation_idx
    ON installation_rehost_authorization (installation_id, created_at DESC);

-- La autorización no puede sobrevivir al challenge de enrollment que la habilita.
CREATE FUNCTION validate_rehost_expiry() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    request_expires TIMESTAMPTZ;
BEGIN
    SELECT expires_at INTO request_expires
    FROM enrollment_request
    WHERE id = NEW.enrollment_request_id;
    IF request_expires IS NULL THEN
        RAISE EXCEPTION 'enrollment_request inexistente';
    END IF;
    IF NEW.expires_at > request_expires THEN
        RAISE EXCEPTION 'rehost expires_at excede enrollment_request.expires_at';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER installation_rehost_expiry_check
BEFORE INSERT OR UPDATE OF expires_at, enrollment_request_id
ON installation_rehost_authorization
FOR EACH ROW EXECUTE FUNCTION validate_rehost_expiry();
