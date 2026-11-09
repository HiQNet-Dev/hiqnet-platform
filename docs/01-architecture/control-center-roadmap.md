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

- **Estado: CERRADO** (integrado en `main` como `00a6baa`, 2026-11-09; `0008` validada con PostgreSQL 17 real).
- Vista global staff con endpoints administrativos explícitos.
- Paginación/filtros server-side, 404 neutral.
- Sin reutilizar endpoints comerciales.
- Incluye `organization_product` (qué organización usa qué producto) cuando exista la vista de organizaciones.

## C5 — POS control module

- **Estado: CERRADO** (integrado en `main` como `8a78105`, 2026-11-09, mediante PR #20; `0009` validada con PostgreSQL 17 real y CI de `main`).
- Installation registry y read models (list/detail current-only).
- Organization ↔ Installation ↔ Product.
- Enrollment product-aware (`product_key → product_id`) con atribución write-once.
- Suspend / reactivate administrativos.
- Rehost con doble autoridad y autorización ligada a `enrollment_request_id`.
- Capabilities `installations.read` / `installations.manage` / `installations.rehost`.
- Staff audit y security events.
- Control API y contrato OpenAPI.
- Control Center Installations UI (list/detail, filtros, paginación, rehost).
- Fuera de C5: billing, pagos, sync control, releases, deployments, terminal remoto, consola de BD e impersonación de clientes.

## C6 — Audit / security center

- **Estado: NO INICIADO / PENDIENTE.**
- Pantalla de auditoría staff, eventos de seguridad, filtros.
- Sin datos inventados; sin secretos.

## C7 — Environments / releases / deployments

- **Estado: NO INICIADO / PENDIENTE.**
- Sólo cuando exista infraestructura real que representar.
- Sin botones de despliegue libre ni consolas productivas.

## Fase Beta — Platform 0.1.0

La Beta es transversal a C1–C6 y su alcance queda definido en el [Backlog Beta](../02-development/platform-beta-backlog.md), pendiente de ratificación. No sustituye las fases C: las consume en este orden:

1. Integrar Portal F01/F02 (PR #12/#13) resolviendo conflictos de workspace/CI.
2. Completar C6.1–C6.4 (auditoría/seguridad).
3. Implementar mutaciones OrganizationProduct.
4. Implementar dominio comercial Plan/Subscription/Entitlement (ADR-0011).
5. Validación integral, runbook y cierre de Beta.

Reglas de numeración:

- `0010`: índices de lectura de C6.
- `0011+`: dominio comercial.
- Si el dominio comercial se adelanta, C6 se renumera y ADR-0010 se actualiza antes de implementar.

C7 sigue **NO INICIADO / DIFERIDO** y no forma parte de Beta.

## Estrategia de PRs

- Fase C0: un PR documental (este).
- Cada fase posterior: PR propio con migración, API, OpenAPI, tests y E2E.
- Customer Portal (PR #12/#13) permanece como superficie de clientes; el Control Center empieza en PR independiente.

## Fuera de roadmap inmediato

Billing global, pagos, terminal arbitrario, consolas de BD productivas, secretos en navegador, impersonación de clientes, acciones destructivas sin diseño, sync control.
