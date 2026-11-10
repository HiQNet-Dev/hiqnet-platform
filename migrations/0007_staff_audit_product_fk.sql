-- C2.1: integridad referencial de la auditoría staff hacia productos.
-- `staff_audit_event.product_id` se creó sin FK en 0005 porque `product` no
-- existía. Ahora que 0006 la crea, se añade la constraint con ON DELETE RESTRICT
-- para que los productos retirados no se eliminen y la auditoría no quede huérfana.

ALTER TABLE staff_audit_event
    ADD CONSTRAINT staff_audit_event_product_id_fkey
    FOREIGN KEY (product_id) REFERENCES product (id) ON DELETE RESTRICT;
