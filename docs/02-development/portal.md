# HiQNet Platform Portal

Portal comercial React para clientes HiQNet. No es el POS, no es HiQNet Admin y no sustituye la autoridad de Platform.

## Stack

- React 19 + TypeScript estricto + Vite.
- React Router.
- TanStack Query.
- Cliente tipado generado desde `openapi/openapi.json` (`openapi-typescript` + `openapi-fetch`).
- Vitest + Testing Library.
- Playwright para E2E real.

## Desarrollo local

Requisitos: Node 24 LTS (`.node-version`), pnpm 11 y la API/PostgreSQL de Platform.

```bash
pnpm install
pnpm --filter @hiqnet/platform-portal api:generate
pnpm --filter @hiqnet/platform-portal dev
```

- Portal: <http://127.0.0.1:5173>
- API: <http://127.0.0.1:8080>
- Swagger: <http://127.0.0.1:8080/docs/>
- Adminer: <http://127.0.0.1:8081/>

Vite hace proxy de `/api` y `/api-docs` hacia `127.0.0.1:8080`. Configura la API local con:

```text
HIQNET_PLATFORM_ALLOWED_ORIGIN=http://127.0.0.1:5173
HIQNET_PLATFORM_ENV=development
HIQNET_PLATFORM_COOKIE_SECURE=false
HIQNET_PLATFORM_ENABLE_DOCS=true
```

No se habilita CORS con `*` ni credentials. La cookie `hiqnet_session` es HttpOnly y el Portal no la lee, no la copia a `localStorage` y no guarda tokens.

## Rutas iniciales

| Ruta | Estado | API real |
| --- | --- | --- |
| `/login` | Implementada | `POST /api/v1/auth/login`, `GET /api/v1/auth/me` |
| `/app/organizations` | Implementada | `GET/POST /api/v1/organizations` |
| `/app/organizations/:id` | Implementada | `GET /api/v1/organizations/{id}` |
| `/app/organizations/:id/members` | Implementada | `GET/POST/PATCH/DELETE /api/v1/organizations/{id}/members` |
| `/account/security` | Implementada | password change, sessions revoke, logout |

`Instalaciones` y `/enroll` quedan fuera de F01 hasta que G4.2-B esté en `main`.

## Scripts

```bash
pnpm --filter @hiqnet/platform-portal api:generate
pnpm --filter @hiqnet/platform-portal api:check
pnpm --filter @hiqnet/platform-portal lint
pnpm --filter @hiqnet/platform-portal typecheck
pnpm --filter @hiqnet/platform-portal test
pnpm --filter @hiqnet/platform-portal build
```

## E2E

Con API, PostgreSQL y Portal arrancados, crea una cuenta sintética con el bootstrap DEV y ejecuta:

```bash
PORTAL_E2E_EMAIL=cuenta@example.test \
PORTAL_E2E_PASSWORD='<contraseña-local>' \
pnpm --filter @hiqnet/platform-portal e2e
```

El test cubre login, organizaciones, creación, detalle, miembros, seguridad de cuenta y logout.

## Seguridad

- Autorización final siempre en Rust/PostgreSQL.
- 401 limpia cache y vuelve a login preservando ruta interna.
- 403 no se oculta; 404 cross-tenant se presenta neutral.
- No se persisten códigos de enrolamiento, contraseñas ni tokens.
- Los errores de API se traducen en una capa central.

## Backend gaps conocidos

| Gap | Impacto UX | API actual | Propuesta futura | Prioridad |
| --- | --- | --- | --- | --- |
| Miembros sólo exponen `account_id` | No se puede buscar por correo ni mostrar nombre | `GET /organizations/{id}/members` | Invitación/búsqueda por email y proyección pública | Alta |
| No hay edición de nombre de organización | El owner no puede renombrar desde Portal | Sólo `POST` y `GET` | `PATCH /organizations/{id}` | Media |
| No hay estado de Trial/plan | No se puede mostrar contexto comercial real | Sin endpoint | G4.2-C | Media |
| Instalaciones/enrolamiento | Sección oculta hasta integrar G4.2-B | PR #11 | Portal F02 | Alta |
