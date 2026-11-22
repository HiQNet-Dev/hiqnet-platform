# Staging en Railway — HiQNet Platform

**Estado:** entorno de **staging temporal**, no producción. Desplegado el 2026-11-22 desde el commit `37ee97f` (rama `feat/platform-railway-staging`).

## Topología real

```text
Internet
  ├── https://control-center-staging-a8a059c3.up.railway.app  → Caddy → /api/* ─┐
  └── https://customer-portal-staging-ef53bf32.up.railway.app → Caddy → /api/* ─┤
                                                                                │
                                                     red privada Railway (railway.internal)
                                                                                │
                                                        platform-api.railway.internal:8080
                                                                                │
                                                              postgres-17.railway.internal:5432
```

La API también expone `https://platform-api-staging-c8003104.up.railway.app` para diagnóstico y para una futura integración nativa de POS. Los navegadores **no** la usan: cada frontend llama a `/api/*` en su propio origen.

## Recursos Railway

| Recurso | Valor |
|---|---|
| Workspace | Gael Navarro's Projects |
| Proyecto | HiQNet Platform — `3c9a72e5-5bf8-4265-8e65-079f9b0887d5` |
| Entorno | staging — `644f4aa3-ae1a-4d11-aa7c-7e91a4ed3c81` |
| `postgres-17` | `8cd8fb02-b19a-47ab-9612-7bd1b48706f3`, `postgres-ssl:17` |
| `platform-api` | `f90e6f90-1a8b-44ac-b929-a5b60dfb9d3a` |
| `control-center` | `ccd1900b-c7e7-466b-a7a2-b4af71f21b93` |
| `customer-portal` | `65ef3f4c-c0b4-42e6-beaf-499bc4948602` |
| Volumen PostgreSQL | `postgres-17-volume`, 5 GB, montado en `/var/lib/postgresql/data` |

## Variables (sólo nombres)

`platform-api`: `DATABASE_URL` (referencia a `postgres-17.DATABASE_URL`), `HIQNET_PLATFORM_ENV`, `HIQNET_PLATFORM_COOKIE_SECURE`, `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE`, `HIQNET_PLATFORM_ALLOW_LICENSE_ISSUANCE`, `HIQNET_PLATFORM_ENABLE_DOCS`, `HIQNET_PLATFORM_SESSION_TTL_HOURS`, `HIQNET_PLATFORM_STAFF_SESSION_TTL_HOURS`, `HIQNET_PLATFORM_AUTH_CONCURRENCY`, `HIQNET_PLATFORM_ENROLLMENT_TTL_SECONDS`, `HIQNET_PLATFORM_ALLOWED_ORIGIN`, `HIQNET_PLATFORM_ENROLLMENT_VERIFICATION_URI`, `RUST_LOG`, `PORT`.

`control-center` y `customer-portal`: `RAILWAY_DOCKERFILE_PATH`, `PLATFORM_API_UPSTREAM`.

No hay `HIQNET_PLATFORM_SIGNING_KEY_B64` ni `HIQNET_PLATFORM_ADMIN_TOKEN`.

## Perfil de seguridad

`HIQNET_PLATFORM_ENV=production`, cookies `Secure`, emisión DEV y comercial desactivadas, `/docs` cerrado, bootstrap desactivado en el servicio, allowlist exacta de orígenes con los dos dominios. Sin CORS, sin wildcard, sin reescritura de `Origin`.

## Procedimiento de despliegue

El servicio no está conectado a GitHub: `railway up` sube el árbol de trabajo. Sólo equivale al commit exacto si el worktree está limpio; verificar antes de desplegar. No hay promoción por digest.

```bash
railway link -p 3c9a72e5-5bf8-4265-8e65-079f9b0887d5 -e 644f4aa3-ae1a-4d11-aa7c-7e91a4ed3c81
railway up . -s platform-api -d -y --path-as-root
railway up . -s control-center -d -y --path-as-root
railway up . -s customer-portal -d -y --path-as-root
```

`--path-as-root` es necesario porque el checkout es un worktree de Git. Las imágenes se construyen desde `Dockerfile` (API) y `deploy/railway/{control-center,portal}/Dockerfile`.

## Verificación

| Comprobación | Resultado esperado |
|---|---|
| `GET /health` | 200 `{"status":"ok"}` |
| `GET /ready` | 200 `database=up` |
| `GET /docs` | 404 |
| Frontends `/control/login` y `/login` | 200, SPA fallback |
| `/api/*` en cada frontend | responde la API del mismo origen |
| Cookies | `HttpOnly`, `Secure`; staff `SameSite=Strict`, comercial `SameSite=Lax` |
| `/healthz` en cada frontend | 200 sólo si la API responde `/ready` |

## Migraciones

SQLx aplica las migraciones al arrancar. Estado verificado: **12 migraciones** (`1`–`12`), 30 tablas.

## Respaldo

Railway no expone aún backup automático utilizable desde CLI para este plan. Procedimiento manual verificado: `pg_dump -Fc` sobre `DATABASE_PUBLIC_URL`, `pg_restore --list` y restauración en una base PostgreSQL 17 temporal y aislada. El dump no se versiona.

## Reversión

1. `railway deployment list` y `railway redeploy` del deployment anterior, o `railway down` si el último es el defectuoso.
2. La base no se revierte automáticamente: las migraciones son aditivas.
3. Si hay que restaurar datos, usar el dump en una base nueva y cambiar `DATABASE_URL`; nunca sobrescribir staging sin autorización.

## Limitaciones conocidas

- Entorno temporal: dominios `*.up.railway.app`, sin dominio corporativo.
- Sin clave de firma Ed25519 ni emisión de licencias; P3 lo aborda.
- Cuentas QA creadas con las CLI `dev-bootstrap-*` ejecutadas localmente contra la base de staging; el servicio mantiene el bootstrap desactivado.
- El allowlist de `Origin` es global y contiene ambos frontends: el origen del portal supera el chequeo de `/api/v1/control/*`. No es un bypass de autenticación ni un CSRF explotable (sin CORS, el preflight bloquea `application/json` cross-origin), pero rompe el aislamiento entre superficies. Separar allowlists antes de ampliar el piloto.
- Sin backup automático; sólo respaldo manual verificado.
- Un solo entorno; producción no existe todavía.
- La API se conecta como `hiqnet_platform`, que es **superusuario** (`rolsuper=true`). Aceptable con datos sintéticos; antes de producción se requiere un rol de ejecución sin superusuario y otro rol para migraciones.
- PostgreSQL conserva un TCP proxy público de Railway (contraseña y TLS). Deshabilitarlo y pasar los respaldos a la red privada antes de producción.
