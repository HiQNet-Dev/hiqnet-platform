-- Registro universal de productos y proyectos del Control Center.
-- No modifica 0001–0005. Las claves (`key`) son estables e inmutables.

CREATE TABLE product (
    id UUID PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN (
        'development', 'active', 'maintenance', 'deprecated', 'retired'
    )),
    product_type TEXT NOT NULL CHECK (product_type IN (
        'desktop', 'saas', 'web', 'mobile', 'service', 'integration', 'internal'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT product_key_format CHECK (key ~ '^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$'),
    CONSTRAINT product_name_nonempty CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    CONSTRAINT product_description_length CHECK (length(description) <= 2000),
    CONSTRAINT product_update_order CHECK (updated_at >= created_at)
);

CREATE INDEX product_status_idx ON product (status);
CREATE INDEX product_type_idx ON product (product_type);

CREATE TABLE project (
    id UUID PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    project_type TEXT NOT NULL CHECK (project_type IN (
        'service', 'application', 'integration', 'infrastructure', 'initiative'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'development', 'active', 'maintenance', 'deprecated', 'retired'
    )),
    product_id UUID REFERENCES product (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT project_key_format CHECK (key ~ '^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$'),
    CONSTRAINT project_name_nonempty CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    CONSTRAINT project_description_length CHECK (length(description) <= 2000),
    CONSTRAINT project_update_order CHECK (updated_at >= created_at)
);

CREATE INDEX project_product_idx ON project (product_id);
CREATE INDEX project_status_idx ON project (status);

-- La clave técnica es estable: no puede cambiar una vez creada.
CREATE FUNCTION reject_product_key_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.key <> OLD.key THEN
        RAISE EXCEPTION 'product.key es inmutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER product_key_immutable
BEFORE UPDATE ON product
FOR EACH ROW EXECUTE FUNCTION reject_product_key_change();

CREATE FUNCTION reject_project_key_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.key <> OLD.key THEN
        RAISE EXCEPTION 'project.key es inmutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER project_key_immutable
BEFORE UPDATE ON project
FOR EACH ROW EXECUTE FUNCTION reject_project_key_change();
