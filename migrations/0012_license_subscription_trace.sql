-- Trazabilidad de emisión comercial y regla de unicidad por suscripción.
--
-- Cada licencia emitida desde un derecho comercial referencia la suscripción
-- que la autoriza (NULL para emisiones DEV históricas). El índice parcial
-- impide dos grants activos para la misma (instalación, suscripción): una
-- emisión duplicada debe resolverse consultando el grant ya emitido, no
-- re-firmando otro documento.

ALTER TABLE issued_license
    ADD COLUMN subscription_id UUID REFERENCES subscription (id) ON DELETE RESTRICT;

CREATE INDEX issued_license_subscription_idx
    ON issued_license (subscription_id);

CREATE UNIQUE INDEX issued_license_installation_subscription_uniq
    ON issued_license (installation_id, subscription_id)
    WHERE subscription_id IS NOT NULL;
