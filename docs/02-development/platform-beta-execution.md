# Ejecución Beta — estado operativo

Última actualización: 2026-11-09

Fuente de verdad: repositorio Git, CI y PRs. Este documento es sólo un índice de recuperación.

## Estado de `main`

```text
697870a  fix(auth): lista de orígenes exactos para CSRF en despliegue multiproducto (#30)
ec46015  chore(beta): cierre de B9 con deps de seguridad y gates portables (#29)
665946b  test(portal): E2E full-stack comercial y CI real (#28)
2adb523  feat(beta): UI comercial, portal read-only y Audit/Security (#27)
c466f06  feat(commercial): núcleo comercial Beta (#26)
7bb82e6  feat(control): mutaciones de OrganizationProduct (#25)
44e9285  feat(control): backend C6 de auditoría y seguridad (#24)
2147156  feat(portal): integrar enrolamiento e instalaciones (F02) (#13)
2046df5  feat(portal): integrar Customer Portal F01 (#12)
565940f  docs(beta): ratify Platform Beta architecture and backlog
```

## Incrementos

| Incremento | Estado | Evidencia |
|---|---|---|
| Fase 0 — Auditoría | **COMPLETADA** | Informe e inventario previos |
| Fase 1 — Ratificación | **COMPLETADA** | `565940f`; ADR-0010, ADR-0011, backlog y seguridad ratificados |
| B1 — Portal F01 | **MERGEADO** | PR #12 → `2046df5`; CI 5/5 |
| B2 — Portal F02 | **MERGEADO** | PR #13 → `2147156`; enrolamiento y proof Ed25519 |
| B3 — C6.1 backend | **MERGEADO** | PR #24 → `44e9285`; migración `0010` |
| B4 — OrganizationProduct | **MERGEADO** | PR #25 → `7bb82e6` |
| B5 — C6.2 frontend | **MERGEADO** | PR #27 → `2adb523` |
| B6 — Dominio comercial | **MERGEADO** | PR #26 → `c466f06`; migración `0011` |
| B7 / B7b — UI y Portal comercial | **MERGEADO** | PR #27 → `2adb523` |
| B8 — E2E full-stack | **MERGEADO** | PR #28 → `665946b`; job CI `Portal E2E full-stack` |
| B9 — Validación Beta | **COMPLETADA** | PR #29 → `ec46015`; PR #30 → `697870a` |
| B10 — Cierre documental | **COMPLETADO** | README, notas de Beta, runbook, backup/restore y transición POS |
| B10.1 — Product Registry administrable | **COMPLETADO** | Alta y edición en Control Center; E2E real en 5 viewports; `read_only` sin mutaciones |
| Publicación de `platform-v0.1.0-beta.1` | **PENDIENTE DE AUTORIZACIÓN** | Requiere push de tag; ver [Notas de Beta](platform-beta-notes.md) §6 |

## B3 — alcance implementado

- Migración `0010_audit_security_read_indexes.sql` con nueve índices.
- `platform-application::audit_security`: read models, filtros, keyset, cursor versionado y sanitización de metadata.
- `platform-storage-postgres::audit_security`: consultas con joins explícitos, keyset y `LIMIT` en PostgreSQL.
- `apps/api::control_audit`: seis endpoints read-only bajo `/api/v1/control/*`, `staffSession`, `audit.read`/`security.read`.
- OpenAPI regenerado y clientes tipados de ambos frontends regenerados.
- Pruebas: aplicación (cursor, límites, sanitización, RBAC), PostgreSQL (keyset, filtros, joins, append-only, índices) y HTTP (401/400/404, metadata saneada, reutilización de cursor).

## B4 — alcance implementado

- `GET /api/v1/control/organizations/{id}/products` (`organizations.read`).
- `POST /api/v1/control/organizations/{id}/products` y `PATCH .../{product_id}` (`organizations.manage`, Origin obligatorio).
- Activación idempotente, desactivación sin DELETE, `activated_at` inmutable.
- Auditoría transaccional `organization_product_activated|deactivated`.
- UI en detalle de organización: asociar producto, activar/desactivar, estados y errores.

## B6/B7/B7b/B5 — alcance implementado

- B6: migración `0011`; planes con features/límites normalizados; suscripciones con trial único, idempotencia, transiciones y renovación; `subscription_event` append-only; resolver de entitlements; API staff y portal read-only.
- B7: páginas Plans y Subscriptions; asociación y entitlements en el detalle de organización.
- B7b: `/organizations/{id}/products` y `/{product_id}` en Customer Portal con membresía y 404 neutral.
- B5: Audit, Platform events y Staff authentication en Control Center con filtros, detalle y metadata saneada.

## B8 — E2E full-stack

- `apps/portal/e2e/commercial.spec.ts` cubre producto sintético no-POS, asociación, plan, suscripción, entitlements, Portal y cross-tenant.
- Job CI `Portal E2E full-stack` levanta PostgreSQL, API Axum y Portal Vite, siembra cuentas y ejecuta Playwright real.
- **Cierre del hueco (PR #30):** `apps/control-center/e2e/full-stack.spec.ts` recorre la UI del Control Center contra API y PostgreSQL reales en **5 viewports** (1920, 1440, 1024, 768, 390), incluyendo una suspensión de suscripción desde el navegador.

## B9 — validación ejecutada

| Gate | Resultado |
|---|---|
| `cargo fmt --all -- --check` | OK |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| `cargo test --workspace` (PostgreSQL 17 real) | 150 passed / 0 failed / 1 ignored |
| Control Center lint/typecheck/test/build/api:check | OK (15 unitarias) |
| Portal lint/format/typecheck/test/build/api:check | OK (9 unitarias) |
| E2E Portal full-stack | 4/4 |
| E2E Control Center full-stack | 5/5 viewports |
| E2E Control Center con API simulada | 155 passed / 5 skipped |
| Migración desde base vacía | 11 migraciones |
| Migración incremental | OK |
| Backup/restore | Conteos coincidentes |
| Keyset con 50 000 eventos | < 0,15 ms por página, `Index Only Scan` |
| Latencia HTTP de listados | 6–12 ms en local |
| Bundles | Control Center 109 kB gzip · Portal 111 kB gzip |
| `cargo audit` | 2 advisories medios aceptados (§7.5 de seguridad) |
| `pnpm audit --prod --audit-level high` | Sin hallazgos |

Correcciones aportadas por B9:

- `time` → 0.3.47 y `ctutils` → 0.4.4 (PR #29).
- Prettier del Portal con `endOfLine: auto` para que `format:check` funcione igual en Windows y CI (PR #29).
- Lista de orígenes exactos en `HIQNET_PLATFORM_ALLOWED_ORIGIN` para permitir dos frontends sobre una API (PR #30).

## B10.1 — Product Registry administrable

- `control-client`: `createProduct` y `updateProduct` tipados desde el contrato OpenAPI.
- `ProductsPage`: formulario de alta (key, name, type, status, description) visible sólo con `products.manage`; validación de cliente y mensajes estables para 400/403/404/409.
- `ProductDetailPage`: formulario de edición con `key` y `type` inmutables, envío sólo de campos cambiados y confirmación modal en cualquier cambio de estado.
- `ControlDialog` extraído a `components/dialog.tsx` y reutilizado por Instalaciones.
- Pruebas: 5 unitarias de validación, 5 E2E simuladas por viewport (capabilities, validación, duplicado, confirmación) y 3 E2E full-stack por viewport (alta y edición reales, auditoría y `read_only` → 403).

## Refactorización estructural (R0–R5)

Incrementos ejecutados tras cerrar B10.1, sin cambios de comportamiento:

| Incremento | Resultado |
|---|---|
| R0 | Auditoría medida y arquitectura modular objetivo ([informe](structural-audit.md), [arquitectura](../01-architecture/modular-architecture.md)) |
| R1 | `apps/api/src/lib.rs`: 6 109 → 103 líneas; módulos `config`, `composition`, `router`, `app_state`, `health`, `licensing`, `commercial_auth`, `organizations`; pruebas en `apps/api/src/tests/` por escenario |
| R2 | `commercial_domain` modularizado en aplicación (model, ports, plans, subscriptions, entitlements, validation, pagination) y persistencia (mappers, sql, helpers, repository, portal) |
| R3 | `enrollment` modularizado; `complete_proof` movido a `proof.rs` conservando transacción y protocolo |
| R4 | Pruebas de persistencia divididas por escenario (`installation_control`, `commercial`, `auth`, `enrollment`) |
| R5 | Verificador `scripts/check_size_policy.py` con línea base y paso de CI |

Métricas finales: **0 archivos de más de 1 000 líneas** (antes 6), archivo más largo **901** (antes 6 109), **150 pruebas** y OpenAPI idéntico.

## Pendiente

1. **Publicar `platform-v0.1.0-beta.1`** cuando se autorice explícitamente. La simulación está validada; no se ha creado tag ni release.
2. **MFA/step-up** antes de ampliar la audiencia.
3. **`sqlx 0.9`** para cerrar los dos advisories medios.
4. **Retención de auditoría y hardening de `TRUNCATE`.**
5. **Contratos de licenciamiento productivo** (custodia de clave, elegibilidad, auditoría de emisión) que se abordan con POS.
6. **Alta y edición de Projects en la UI.** La API ya lo soporta; mismo patrón que Products.

## Reglas de recuperación

- No repetir B1–B10: están en `main`.
- No crear migraciones `0010`/`0011` duplicadas.
- No modificar migraciones `0001–0011`.
- No fusionar sin CI verde.
- No usar force-push sobre ramas publicadas.
- No publicar tags ni releases sin autorización explícita.
