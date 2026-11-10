-- B6: Commercial Core mínimo de Beta.
-- Plan, Subscription y SubscriptionEvent. Sin pagos, facturas ni prorrateo.

CREATE TABLE plan (
    id UUID PRIMARY KEY,
    product_id UUID NOT NULL REFERENCES product (id) ON DELETE RESTRICT,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'active', 'maintenance', 'deprecated', 'retired'
    )),
    is_trial BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT plan_key_format CHECK (key ~ '^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$'),
    CONSTRAINT plan_name_nonempty CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    CONSTRAINT plan_update_order CHECK (updated_at >= created_at),
    UNIQUE (product_id, key)
);

CREATE UNIQUE INDEX plan_one_trial_per_product_idx
    ON plan (product_id) WHERE is_trial;
CREATE INDEX plan_product_status_idx ON plan (product_id, status);

CREATE TABLE plan_feature (
    plan_id UUID NOT NULL REFERENCES plan (id) ON DELETE RESTRICT,
    key TEXT NOT NULL,
    value BOOLEAN NOT NULL,
    PRIMARY KEY (plan_id, key),
    CONSTRAINT plan_feature_key_format CHECK (key ~ '^[a-z0-9]([a-z0-9._-]{0,62}[a-z0-9])?$')
);

CREATE TABLE plan_limit (
    plan_id UUID NOT NULL REFERENCES plan (id) ON DELETE RESTRICT,
    key TEXT NOT NULL,
    value BIGINT NOT NULL CHECK (value >= 0),
    PRIMARY KEY (plan_id, key),
    CONSTRAINT plan_limit_key_format CHECK (key ~ '^[a-z0-9]([a-z0-9._-]{0,62}[a-z0-9])?$')
);

CREATE FUNCTION protect_plan_conditions() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (SELECT 1 FROM subscription WHERE plan_id = OLD.id) THEN
            RAISE EXCEPTION 'plan con subscriptions no admite eliminación física';
        END IF;
        RETURN OLD;
    END IF;

    IF (NEW.key IS DISTINCT FROM OLD.key
        OR NEW.product_id IS DISTINCT FROM OLD.product_id
        OR NEW.is_trial IS DISTINCT FROM OLD.is_trial)
       AND EXISTS (SELECT 1 FROM subscription WHERE plan_id = OLD.id) THEN
        RAISE EXCEPTION 'condiciones de plan contratado son inmutables';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER plan_conditions_immutable
BEFORE UPDATE OR DELETE ON plan
FOR EACH ROW EXECUTE FUNCTION protect_plan_conditions();

CREATE FUNCTION protect_plan_entitlements() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_plan UUID;
BEGIN
    target_plan := CASE WHEN TG_OP = 'DELETE' THEN OLD.plan_id ELSE NEW.plan_id END;
    IF EXISTS (SELECT 1 FROM subscription WHERE plan_id = target_plan) THEN
        RAISE EXCEPTION 'features/limits de plan contratado son inmutables';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER plan_feature_immutable
BEFORE INSERT OR UPDATE OR DELETE ON plan_feature
FOR EACH ROW EXECUTE FUNCTION protect_plan_entitlements();

CREATE TRIGGER plan_limit_immutable
BEFORE INSERT OR UPDATE OR DELETE ON plan_limit
FOR EACH ROW EXECUTE FUNCTION protect_plan_entitlements();

CREATE TABLE subscription (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organization (id) ON DELETE RESTRICT,
    product_id UUID NOT NULL REFERENCES product (id) ON DELETE RESTRICT,
    plan_id UUID NOT NULL REFERENCES plan (id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('active', 'suspended', 'cancelled', 'expired')),
    is_trial BOOLEAN NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    suspended_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    renewed_from_subscription_id UUID REFERENCES subscription (id) ON DELETE RESTRICT,
    idempotency_scope TEXT,
    idempotency_key_hash BYTEA,
    request_hash BYTEA,
    CONSTRAINT subscription_dates CHECK (ends_at > starts_at),
    CONSTRAINT subscription_suspended_at CHECK (
        (status = 'suspended' AND suspended_at IS NOT NULL)
        OR (status <> 'suspended' AND suspended_at IS NULL)
    ),
    CONSTRAINT subscription_cancelled_at CHECK (
        (status = 'cancelled' AND cancelled_at IS NOT NULL)
        OR (status <> 'cancelled' AND cancelled_at IS NULL)
    )
);

CREATE UNIQUE INDEX subscription_one_current_idx
    ON subscription (organization_id, product_id)
    WHERE status IN ('active', 'suspended');
CREATE UNIQUE INDEX subscription_one_trial_idx
    ON subscription (organization_id, product_id)
    WHERE is_trial;
CREATE UNIQUE INDEX subscription_idempotency_idx
    ON subscription (idempotency_scope, idempotency_key_hash)
    WHERE idempotency_key_hash IS NOT NULL;
CREATE INDEX subscription_organization_product_idx
    ON subscription (organization_id, product_id, status);
CREATE INDEX subscription_plan_idx ON subscription (plan_id);

CREATE TABLE subscription_event (
    id UUID PRIMARY KEY,
    subscription_id UUID NOT NULL REFERENCES subscription (id) ON DELETE RESTRICT,
    event TEXT NOT NULL CHECK (event IN (
        'created', 'suspended', 'reactivated', 'cancelled', 'expired', 'renewed'
    )),
    from_status TEXT,
    to_status TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    actor_staff_account_id UUID REFERENCES staff_account (id) ON DELETE RESTRICT,
    reason TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT subscription_event_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object')
);

CREATE INDEX subscription_event_subscription_time_idx
    ON subscription_event (subscription_id, occurred_at DESC, id DESC);

CREATE FUNCTION reject_subscription_event_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'subscription_event es inmutable';
END;
$$;

CREATE TRIGGER subscription_event_append_only
BEFORE UPDATE OR DELETE ON subscription_event
FOR EACH ROW EXECUTE FUNCTION reject_subscription_event_change();
