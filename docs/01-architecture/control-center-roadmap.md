# Control Center — Roadmap incremental

**Estado:** ACCEPTED (C0 ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Relacionado:** [ADR-0004](adr-0004-control-center-boundaries.md)

Cada fase debe compilar, testear, exponer OpenAPI, usar migraciones propias y pasar E2E cuando aplique. No crear un PR gigantesco.

## C0 — Architecture Foundation (esta fase)

- ADR-0004 boundaries.
- ADR-0005 staff identity.
- ADR-0006 staff RBAC.
- ADR-0007 Product/Project model.
- ADR-0008 frontend modules.
- Threat model, RBAC matrix, UI direction, este roadmap.
- **Sin código, sin DDL, sin `apps/control-center`.**

## C1 — Staff identity + sessions

- Migración `0005_staff_identity.sql` (tras aprobación).
- `POST /control/auth/login|logout`, `GET /control/auth/me`.
- Argon2id, sesiones persistidas, cookie `hiqnet_staff_session`, Origin, rate limit, bloqueo.
- Auditoría staff.
- Tests: identidad, revocación, concurrencia, `commercialSession`/`devAdminToken` rechazados.

## C2 — Product registry

- Migración `0006_product_registry.sql`: **Product + Project** con clave estable inmutable y relación 0..1 Project → Product.
- Endpoints `control` de lectura/gestión con RBAC (`products.read`/`products.manage`).
- OpenAPI + tests de RBAC, duplicados, paginación y auditoría.
- `Environment` se difiere a C7; `organization_product` se difiere a C4/C5. El registro C2 no incluye organizaciones, billing ni entitlements.

## C3 — Control Center frontend foundation

- `apps/control-center` (React + TS + Vite).
- Login staff, AppShell, Products index/detail, Organizations index.
- Registry tipado, `controlClient`, query keys namespaced.
- Visual QA (1920/1440/1024/768/390), accesibilidad, E2E.
- CI frontend independiente.

## C4 — Organizations / accounts global view

- Vista global staff con endpoints administrativos explícitos.
- Paginación/filtros server-side, 404 neutral.
- Sin reutilizar endpoints comerciales.
- Incluye `organization_product` (qué organización usa qué producto) cuando exista la vista de organizaciones.

## C5 — POS control module

- Overview, Organizations, Installations del producto POS.
- Licenses/Trial/Subscriptions sólo cuando existan APIs reales.

## C6 — Audit / security center

- Pantalla de auditoría staff, eventos de seguridad, filtros.
- Sin datos inventados; sin secretos.

## C7 — Environments / releases / deployments

- Sólo cuando exista infraestructura real que representar.
- Sin botones de despliegue libre ni consolas productivas.

## Estrategia de PRs

- Fase C0: un PR documental (este).
- Cada fase posterior: PR propio con migración, API, OpenAPI, tests y E2E.
- Customer Portal (PR #12/#13) permanece como superficie de clientes; el Control Center empieza en PR independiente.

## Fuera de roadmap inmediato

Billing global, pagos, terminal arbitrario, consolas de BD productivas, secretos en navegador, impersonación de clientes, acciones destructivas sin diseño, sync control.
