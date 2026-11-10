# HiQNet Platform

Infraestructura administrativa y comercial de HiQNet: **aplicación 100 % web** con Control Center, Customer Portal y servicios de núcleo sobre una API única.

**Estado:** Beta `0.1.0-beta.1` cerrada (pendiente de publicación de etiqueta).
**Primer producto consumidor:** HiQNet POS.

## Qué incluye

| Superficie | Audiencia | Contenido |
|---|---|---|
| **Control Center** | Personal HiQNet | Productos, proyectos, organizaciones, cuentas, instalaciones, planes, suscripciones, auditoría y eventos de seguridad |
| **Customer Portal** | Clientes | Organizaciones, miembros, instalaciones, productos contratados, plan, suscripción y derechos efectivos |
| **Core Services** | Ambos | Identidad staff y comercial, RBAC, organizaciones, catálogo, suscripciones, entitlements, licenciamiento y auditoría |

No es una aplicación de escritorio, móvil nativa ni PWA. No hay Tauri, Electron ni instaladores.

## Stack

Rust 1.99 · Axum 0.8 · SQLx 0.8 · PostgreSQL 17 · React 19 · TypeScript · Vite · OpenAPI 3.1 · Playwright.

## Estructura del repositorio

```text
apps/
  api/                 API Axum (HTTP, composición, OpenAPI)
  control-center/      SPA de staff
  portal/              SPA de clientes
crates/
  platform-core/       Dominio y reglas
  platform-application/ Casos de uso
  platform-storage-postgres/ Adaptador SQLx
  platform-licensing/  Firma Ed25519 de licencias
migrations/            Migraciones SQLx incrementales (0001–0011)
openapi/               Contrato OpenAPI 3.1
scripts/               Validaciones de repositorio y release
docs/                  Arquitectura, desarrollo y operaciones
```

## Arranque local

```bash
cp .env.example .env          # define una contraseña local; nunca la publiques
docker compose up -d postgres
docker compose up --build -d api
pnpm install
pnpm --filter @hiqnet/platform-portal dev        # http://127.0.0.1:5173
pnpm --filter @hiqnet/control-center dev         # http://127.0.0.1:5174
```

`GET /health` prueba el proceso HTTP; `GET /ready` exige un roundtrip real a PostgreSQL. No son equivalentes.

Detalle completo en [Desarrollo local](docs/02-development/local.md).

## Validación

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                        # requiere TEST_DATABASE_URL y las URL de upgrade

pnpm --filter @hiqnet/control-center lint
pnpm --filter @hiqnet/control-center typecheck
pnpm --filter @hiqnet/control-center test
pnpm --filter @hiqnet/control-center build
pnpm --filter @hiqnet/platform-portal lint
pnpm --filter @hiqnet/platform-portal format:check
pnpm --filter @hiqnet/platform-portal typecheck
pnpm --filter @hiqnet/platform-portal test
pnpm --filter @hiqnet/platform-portal build
```

CI ejecuta además E2E reales con API Axum y PostgreSQL: el recorrido comercial del Portal y el del Control Center en cinco viewports.

## Documentación

- [Arquitectura rectora](docs/01-architecture/platform.md)
- [ADR-0010 · Audit & Security Center](docs/01-architecture/adr-0010-audit-security-center.md)
- [ADR-0011 · Dominio comercial](docs/01-architecture/adr-0011-commercial-domain.md)
- [Backlog cerrado de Beta](docs/02-development/platform-beta-backlog.md)
- [Seguridad de Beta](docs/02-development/platform-beta-security.md)
- [Notas de la Beta 0.1.0](docs/02-development/platform-beta-notes.md)
- [Despliegue](docs/03-operations/deployment.md) · [Backup y restauración](docs/03-operations/backup-restore.md)
- [Transición a HiQNet POS](docs/02-development/platform-transition-pos.md)

## Reglas del proyecto

- Las migraciones publicadas no se reescriben; sólo se añaden incrementales.
- La autorización se decide en el backend; la UI sólo refleja permisos.
- Nunca se versionan `.env`, dumps, tokens ni claves.
- Los PR requieren CI verde antes de fusionar.
- No se publican tags ni releases sin validación y autorización.
