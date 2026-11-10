# Ejecución Beta — estado operativo

Última actualización: 2026-11-09

Fuente de verdad: repositorio Git, CI y PRs. Este documento es sólo un índice de recuperación.

## Estado de `main`

```text
44e9285  feat(control): backend C6 de auditoría y seguridad
2147156  feat(portal): integrar enrolamiento e instalaciones (F02)
2046df5  feat(portal): integrar Customer Portal F01
565940f  docs(beta): ratify Platform Beta architecture and backlog
```

## Incrementos

| Incremento | Estado | Evidencia |
|---|---|---|
| B1 — Portal F01 | **MERGEADO** | PR #12 → `2046df5`; CI 4/4; E2E real del Portal |
| B2 — Portal F02 | **MERGEADO** | PR #13 → `2147156`; CI 4/4; E2E synthetic POS + proof Ed25519 |
| B3 — C6.1 backend | **MERGEADO** | PR #24 → `44e9285`; CI 4/4; migración `0010`, endpoints y pruebas |
| B4 — OrganizationProduct | **EN RAMA / VALIDADO LOCAL** | Backend GET/POST/PATCH, UI en organización, auditoría e idempotencia |
| B5 — C6.2 frontend | NO INICIADO | Depende de B3 |
| B6 — Dominio comercial | NO INICIADO | Depende de B3/B4 |
| B7 / B7b — UI y Portal comercial | NO INICIADO | Depende de B6 |
| B8 — E2E full-stack | NO INICIADO | Depende de B4/B6/B7b |
| B9 — Validación Beta | NO INICIADO | Depende de B8 |
| B10 — Cierre Beta | NO INICIADO | Depende de B9 |

## B3 — alcance implementado

- Migración `0010_audit_security_read_indexes.sql` con nueve índices.
- `platform-application::audit_security`: read models, filtros, keyset, cursor versionado y sanitización de metadata.
- `platform-storage-postgres::audit_security`: consultas con joins explícitos, keyset y `LIMIT` en PostgreSQL.
- `apps/api::control_audit`: seis endpoints read-only bajo `/api/v1/control/*`, `staffSession`, `audit.read`/`security.read`.
- OpenAPI regenerado y clientes tipados de ambos frontends regenerados.
- Pruebas: aplicación (cursor, límites, sanitización, RBAC), PostgreSQL (keyset, filtros, joins, append-only, índices) y HTTP (401/400/404, metadata saneada, reutilización de cursor).

## Validaciones B3

```text
cargo fmt --all -- --check              OK
cargo clippy --workspace --all-targets -- -D warnings  OK
cargo test --workspace                  OK (134 passed / 1 ignored)
Control Center lint/typecheck/test/build OK (14 tests)
Portal lint/typecheck/test/build         OK (9 tests)
OpenAPI snapshot generado y clientes api:check OK
```

## B4 — alcance implementado

- `GET /api/v1/control/organizations/{id}/products` (`organizations.read`).
- `POST /api/v1/control/organizations/{id}/products` y `PATCH .../{product_id}` (`organizations.manage`, Origin obligatorio).
- Activación idempotente, desactivación sin DELETE, `activated_at` inmutable.
- Auditoría transaccional `organization_product_activated|deactivated`.
- UI en detalle de organización: asociar producto, activar/desactivar, estados y errores.
- Pruebas de aplicación, PostgreSQL y HTTP (RBAC, 404 neutral, idempotencia, auditoría).

## Validaciones B4

```text
cargo fmt --all -- --check                              OK
cargo clippy --workspace --all-targets -- -D warnings  OK
cargo test --workspace                                  OK (140 passed / 1 ignored)
Control Center lint/typecheck/test/build                OK (15 tests)
Portal lint/typecheck/test/build                        OK (9 tests)
OpenAPI snapshot y clientes api:check                   OK
```

## Pendiente inmediato

1. Publicar la rama de B4 y abrir PR.
2. Verificar CI.
3. Fusionar B4 con squash.
4. Continuar B6 Commercial Core (B5 puede ir después o en paralelo).

## Reglas de recuperación

- No repetir B1/B2: ya están en `main`.
- No crear migraciones `0010` duplicadas.
- No modificar migraciones `0001–0009`.
- No fusionar sin CI verde.
- No usar force-push sobre ramas publicadas.
