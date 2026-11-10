-- C4: asociación histórica Organization ↔ Product.
-- `activated_at` registra sólo la primera activación y nunca se reescribe.

CREATE TABLE organization_product (
    organization_id UUID NOT NULL REFERENCES organization (id) ON DELETE RESTRICT,
    product_id UUID NOT NULL REFERENCES product (id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    activated_at TIMESTAMPTZ NOT NULL,
    entitlements JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (organization_id, product_id),
    CONSTRAINT organization_product_entitlements_object
        CHECK (jsonb_typeof(entitlements) = 'object')
);

CREATE INDEX organization_product_product_idx ON organization_product (product_id);
CREATE INDEX organization_product_organization_status_idx
    ON organization_product (organization_id, status);

CREATE FUNCTION protect_organization_product_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'organization_product no admite eliminación física';
    END IF;

    IF NEW.organization_id IS DISTINCT FROM OLD.organization_id
       OR NEW.product_id IS DISTINCT FROM OLD.product_id
       OR NEW.activated_at IS DISTINCT FROM OLD.activated_at THEN
        RAISE EXCEPTION 'organization_product identity and activated_at are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER organization_product_identity_immutable
BEFORE UPDATE OR DELETE ON organization_product
FOR EACH ROW EXECUTE FUNCTION protect_organization_product_identity();
