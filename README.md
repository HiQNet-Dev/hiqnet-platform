# HiQNet Platform

Control plane de HiQNet: registro de instalaciones, catálogo de planes y emisión
de autorizaciones comerciales firmadas para POS locales.

> Estado: **fundación técnica 0.1.0**. Billing real, sincronización Pro y portal
> administrativo siguen pendientes.

## Ejecutar en desarrollo

```bash
cp .env.example .env
docker compose up -d postgres
cargo run -p hiqnet-platform-api
```

La API queda en `http://127.0.0.1:8080`. Endpoints:

- `GET /health`
- `GET /api/v1/plans`
- `POST /api/v1/installations` (requiere `Authorization: Bearer <token>`)
- `GET /api/v1/installations/{id}`
- `POST /api/v1/installations/{id}/trial`
- `POST /api/v1/installations/{id}/licenses` (solo desarrollo, requiere clave de firma)

## Validaciones

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

El contrato HTTP está en `openapi/openapi.json` (OpenAPI 3.1).

## Seguridad

- Las mutaciones exigen token administrativo (`HIQNET_PLATFORM_ADMIN_TOKEN`).
- Sin token configurado, se rechazan con 401 (deny-by-default).
- La clave privada de emisión solo puede existir en Platform; el POS solo recibe
  la clave pública.
- Las credenciales de desarrollo del `.env.example` no son productivas.
