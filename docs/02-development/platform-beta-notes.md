# HiQNet Platform — Notas de la Beta 0.1.0

**Estado:** Beta cerrada, pendiente de publicación de la etiqueta.
**Versión:** `0.1.0-beta.1`
**Etiqueta prevista:** `platform-v0.1.0-beta.1`
**Fecha:** 2026-11-09
**Relacionado:** [Arquitectura](../01-architecture/platform.md) · [Backlog Beta](platform-beta-backlog.md) · [Seguridad Beta](platform-beta-security.md) · [Despliegue](../03-operations/deployment.md) · [Transición a POS](platform-transition-pos.md)

## 1. Qué es esta Beta

HiQNet Platform Beta `0.1.0-beta.1` es la **infraestructura administrativa web** de HiQNet, exclusivamente web y multiproducto. Incluye tres superficies:

- **Control Center**: aplicación privada para personal staff.
- **Customer Portal**: aplicación para clientes con sesión comercial.
- **Core Services**: API Axum con PostgreSQL que consume ambos frontends.

Tecnologías: Rust 1.99.1, Axum 0.8, SQLx 0.8, PostgreSQL 17, React 19, TypeScript, Vite, OpenAPI 3.1 y Playwright.

No es una aplicación de escritorio, móvil nativa ni PWA. No hay Tauri, Electron, React Native ni instaladores.

## 2. Alcance entregado

| Capacidad | Estado |
|---|---|
| Identidad staff, login, logout, sesión revocable | Entregado |
| RBAC staff con 17 capabilities y 6 roles | Entregado |
| Product Registry (listado, detalle, alta y edición) | Entregado |
| Project Registry (registro asociado a producto) | Entregado |
| Organizaciones y cuentas comerciales (consulta y detalle) | Entregado |
| Asociación organización-producto (activar/desactivar) | Entregado |
| Installation Management (listado, detalle, suspender, reactivar, rehost) | Entregado |
| Enrolamiento con proof Ed25519 | Entregado |
| Planes comerciales por producto con features y límites | Entregado |
| Suscripciones con estados, transiciones, trial único, renovación e idempotencia | Entregado |
| Resolución de entitlements efectivos en backend | Entregado |
| Audit & Security Center (solo lectura) | Entregado |
| Customer Portal: organizaciones, miembros, instalaciones, productos contratados, plan, suscripción y entitlements | Entregado |
| Licenciamiento técnico Ed25519 (`LicenseGrant` v1) | Conservado, emisión productiva **no habilitada** |

## 3. Fuera de alcance

No forma parte de esta Beta y no debe considerarse pendiente de último momento:

- Pagos, checkout, cobros automáticos, conciliación y facturación fiscal.
- Autoservicio de alta de clientes o de cambio de plan.
- C7 (environments, releases, deployments).
- Sincronización cloud.
- Administración de repositorios, pipelines o tareas de desarrollo.
- Control remoto, SQL remoto, acceso a filesystem o impersonación.
- SIEM, scoring de riesgo, geolocalización, reputación de dispositivo.
- MFA/step-up.
- Aplicaciones nativas o modo offline del navegador.

## 4. Perfiles de exposición

La Beta puede desplegarse en dos perfiles. **Un riesgo aceptado para uno no se hereda al otro.**

| Perfil | Audiencia | Condición |
|---|---|---|
| **Interna restringida** | Sólo staff HiQNet, red controlada | Cumplir [§5.1 de seguridad](platform-beta-security.md) |
| **Externa limitada** | Clientes invitados, tenants acotados | Cumplir §5.1 + §5.2 y aceptar por escrito el riesgo S1 (MFA pendiente) |
| Producción pública | Público general | **No es esta Beta**; exige MFA/step-up y hardening adicional |

Estado de los controles: HTTPS/TLS, `Secure`, `HttpOnly`, `SameSite`, `Origin` exacto, revocación de sesión, RBAC, aislamiento por organización, auditoría append-only y metadata saneada están implementados y verificados. MFA y la reducción de `security.read`/`audit.read` quedan pendientes y condicionan la exposición externa.

## 5. Limitaciones conocidas

Se documentan para que no se descubran en producción:

1. **`security.read` y `audit.read` las tienen todos los roles.** Riesgo de confidencialidad interna, acotado por la sanitización de metadata.
2. **`security.manage` no tiene superficie.** Existe en la matriz y se consume en una fase posterior.
3. **`TRUNCATE` no está bloqueado por los triggers append-only.** Mitigado con privilegios de base y backups.
4. **Sin retención ni particionado de auditoría.** El volumen crece sin política automática.
5. **Rate limiters en memoria y por instancia.** Un despliegue multi-instancia necesitará un limitador compartido.
6. **`staff_auth_event` no registra todos los eventos posibles.** Faltan writers para `session_revoked`, `account_locked` y `password_changed`.
7. **Sin PITR ni backups automáticos.** Depende del host; ver [Backup y restauración](../03-operations/backup-restore.md).
8. **Dos advisories medios de `cargo audit` aceptados** (`libsqlite3-sys`, `rsa`): aparecen como dependencias opcionales en `Cargo.lock` sin ruta de compilación. Se cierran al migrar a `sqlx 0.9`.
9. **La CSP propuesta no se validó contra un proxy real.** Debe verificarse en el despliegue.
10. **Emisión productiva de licencias deshabilitada.** `ALLOW_DEV_ISSUANCE=false`; la custodia de clave se decide con POS.
11. **Sin MFA/step-up.** Aceptado para staff interno; requisito para ampliar audiencia.

## 6. Identificación y publicación de la versión

Esquema vigente (`Cargo.toml` + `scripts/validate_release.py` + `.github/workflows/release-platform.yml`):

- Versión del workspace: `0.1.0`.
- Tag válido: `platform-v<SemVer>` cuyo núcleo coincida con la versión del workspace.
- Prerelease SemVer obligatorio para la Beta: `0.1.0-beta.1`.
- Sin metadata `+build`.

Simulación verificada:

```bash
RELEASE_EVENT=workflow_dispatch \
RELEASE_REF=refs/heads/main \
RELEASE_VERSION=0.1.0-beta.1 \
python scripts/validate_release.py
# tag=platform-v0.1.0-beta.1
# prerelease=true
```

La simulación anterior con `0.1.0` (`prerelease=false`) correspondía a una versión final y **no** identifica la Beta.

**La publicación no se ha ejecutado.** Crear el tag dispara `release-platform.yml`, que construye la imagen y abre un GitHub Release con `prerelease: true`. Publicar la etiqueta es un paso que requiere autorización explícita y un commit validado. Mientras no exista un despliegue autorizado, la Beta no es "públicamente disponible".

## 7. Cómo validar la Beta

Backend y persistencia:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
TEST_DATABASE_URL=... TEST_UPGRADE_DATABASE_URL=... TEST_AUTH_UPGRADE_DATABASE_URL=... \
  cargo test --workspace
python -m unittest discover -s scripts -p "test_*.py"
python scripts/check_repository.py
cargo audit
pnpm audit --prod --audit-level high
```

Frontends:

```bash
pnpm install --frozen-lockfile
pnpm --filter @hiqnet/control-center lint
pnpm --filter @hiqnet/control-center typecheck
pnpm --filter @hiqnet/control-center test
pnpm --filter @hiqnet/control-center build
pnpm --filter @hiqnet/control-center api:check
pnpm --filter @hiqnet/platform-portal lint
pnpm --filter @hiqnet/platform-portal format:check
pnpm --filter @hiqnet/platform-portal typecheck
pnpm --filter @hiqnet/platform-portal test
pnpm --filter @hiqnet/platform-portal build
pnpm --filter @hiqnet/platform-portal api:check
```

E2E full-stack (API Axum + PostgreSQL reales, sin mocks):

```bash
# Requiere API en 8080 con ALLOWED_ORIGIN incluyendo 5173 y 5174, y las cuentas sembradas.
PORTAL_E2E_EMAIL=... PORTAL_E2E_PASSWORD=... \
PLATFORM_STAFF_EMAIL=... PLATFORM_STAFF_PASSWORD=... \
  pnpm --filter @hiqnet/platform-portal e2e

PLATFORM_STAFF_EMAIL=... PLATFORM_STAFF_PASSWORD=... \
  pnpm --filter @hiqnet/control-center exec playwright test e2e/full-stack.spec.ts
```

## 8. Evidencia de validación

| Gate | Resultado |
|---|---|
| `cargo fmt --check` / `clippy -D warnings` | Verde |
| `cargo test --workspace` (PostgreSQL 17 real) | 150 pruebas, 0 fallos, 1 ignorada |
| Control Center lint/typecheck/test/build/api:check | Verde (20 unitarias) |
| Portal lint/format/typecheck/test/build/api:check | Verde (9 unitarias) |
| E2E Portal full-stack | 4/4 (login, cuenta, enrolamiento, comercial y aislamiento) |
| E2E Control Center full-stack | 15/15 en 5 viewports (1920, 1440, 1024, 768, 390) |
| E2E Control Center con API simulada | 180 pruebas, 15 omitidas (las full-stack sin credenciales) |
| Migración desde base vacía | 11 migraciones aplicadas |
| Migración incremental | Verde |
| Backup/restore | Ensayado con conteos coincidentes |
| Consultas keyset con 50 000 eventos | < 0,15 ms por página, `Index Only Scan` |
| Latencia HTTP de listados | 6–12 ms en local |
| Bundles de producción | Control Center 109 kB gzip · Portal 111 kB gzip |
| `cargo audit` | 2 advisories medios aceptados y justificados |
| `pnpm audit --prod --audit-level high` | Sin hallazgos |

Las pruebas de interfaz se ejecutaron con **Playwright CLI** contra servicios reales. No hubo Playwright MCP disponible en el entorno; la sustitución queda registrada.

## 9. Siguiente paso

Con la Beta cerrada, el desarrollo principal se traslada a **HiQNet POS**. Platform entra en modo mantenimiento: sólo parches de seguridad, defectos y la integración necesaria para POS. Ver [Transición a POS](platform-transition-pos.md).
