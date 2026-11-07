# API HTTP, OpenAPI 3.1 y Swagger UI (P01)

## Iniciar y consultar

Sigue primero [desarrollo local](local.md): copia `.env.example` a `.env`, configura contraseña DEV local (conserva la ya rotada si el volumen existe), ejecuta `docker compose up -d postgres` y luego `docker compose up --build -d api`. PostgreSQL y API sólo publican puertos de loopback en Compose. Las migraciones SQLx se aplican durante el arranque de API; no modifiques `migrations/0001_init.sql`.

Con `HIQNET_PLATFORM_ENABLE_DOCS=true` en `.env`, abre <http://127.0.0.1:8080/docs/> (la ruta `/docs` redirige a `/docs/`). El contrato JSON está en <http://127.0.0.1:8080/api-docs/openapi.json>. Si la variable no está configurada o vale algo distinto de `true`, **ambas rutas están deshabilitadas**. Compose local usa `true` por defecto; en otros despliegues habilítalo sólo mediante configuración explícita, con control de acceso de red si procede. Ni Swagger ni OpenAPI incluyen token alguno.

Para iniciar con `cargo run -p hiqnet-platform-api`, detén antes el servicio `api` de Compose, configura `DATABASE_URL` como se indica en [desarrollo local](local.md) y establece `HIQNET_PLATFORM_ENABLE_DOCS=true` explícitamente si deseas Swagger. No se ha probado esta guía en host Omarchy.

Rutas existentes (sin query parameters):

| Método | Ruta | Autenticación | Éxito | Casos frecuentes |
| --- | --- | --- | --- | --- |
| GET | `/health` | Pública | 200 | Estado del proceso, no de BD |
| GET | `/ready` | Pública | 200 | 503 si falla PostgreSQL |
| GET | `/api/v1/plans` | Pública | 200 | Catálogo técnico, sin precios ni disponibilidad comercial |
| POST | `/api/v1/installations` | Bearer DEV | 201 | 400 etiqueta vacía; 404 organización inexistente; 409 etiqueta duplicada; 415/422 JSON inválido; 503 BD |
| GET | `/api/v1/installations/{id}` | Bearer DEV | 200 | 400 UUID inválido; 404 no existe; 503 BD |
| POST | `/api/v1/installations/{id}/trial` | Bearer DEV | 200 | 404 no existe; 409 ya activado; 503 BD |
| GET | `/api/v1/installations/{id}/licenses` | Bearer DEV | 200 | UUID inexistente: `[]`; 400 UUID inválido; 503 BD |
| POST | `/api/v1/installations/{id}/licenses` | Bearer DEV | 201 | 400 plan/vigencia inválidos; 404 no existe; 503 emisión DEV deshabilitada o BD |

En todas las rutas protegidas, token ausente, incorrecto o no configurado devuelve 401. Para `POST` se exige `Content-Type: application/json`. Errores de negocio, entrada y almacenamiento devuelven JSON `{"code":"not_found","message":"no se encontró el recurso solicitado"}` (con códigos y mensajes según caso); detalles SQL, claves y trazas no se incluyen. `/ready` devuelve su propio esquema de estado. Las fechas de DTO HTTP usan RFC 3339; `grant_json` y `features_json` son cadenas que contienen JSON serializado. Los identificadores UUID no representan autenticación. No se configura CORS permisivo; no presupongas acceso desde cualquier origen de navegador.

## Ejemplos DEV

Una consulta pública no necesita token:

```sh
curl http://127.0.0.1:8080/health
# {"status":"ok","version":"0.1.0"}
curl http://127.0.0.1:8080/api/v1/plans
```

Para rutas protegidas, genera un token aleatorio **sólo localmente**, configura `HIQNET_PLATFORM_ADMIN_TOKEN` en `.env` (no lo subas a Git) y reinicia `api` con `docker compose up -d api`. Introduce el valor en **Authorize → devAdminToken** en Swagger UI; usa la misma credencial en `Authorization: Bearer <TOKEN_DEV_LOCAL>` con curl. No pegues la credencial real en PR, logs ni ejemplos. No se trata de identidad de usuarios: es una protección administrativa temporal DEV, no un mecanismo multitenant ni de producción.

Desde P02.1, la organización de una instalación **debe existir**: un UUID enviado por el cliente no crea organización ni concede membresía. Para probar el endpoint técnico antes de P02.2, crea exclusivamente en tu base **local DEV** una organización sintética (sin cuenta ni membresías). Nunca ejecutes este ejemplo sobre una base compartida o productiva:

```sh
docker compose exec postgres psql -U hiqnet -d hiqnet -c "INSERT INTO organization (id, display_name, status, created_at) VALUES ('123e4567-e89b-42d3-a456-426614174001', 'Organización DEV sintética', 'active', now()) ON CONFLICT (id) DO NOTHING;"
```

```sh
curl -i -X POST http://127.0.0.1:8080/api/v1/installations \
  -H 'Authorization: Bearer <TOKEN_DEV_LOCAL>' \
  -H 'Content-Type: application/json' \
  -d '{"installation_id":"123e4567-e89b-42d3-a456-426614174000","organization_id":"123e4567-e89b-42d3-a456-426614174001","device_label":"Caja de pruebas"}'
# 201: {"id":"123e4567-e89b-42d3-a456-426614174000","organization_id":"123e4567-e89b-42d3-a456-426614174001","device_label":"Caja de pruebas","status":"active","registered_at":"<FECHA_RFC3339>","trial_started_at":null,"trial_ends_at":null}
curl -i http://127.0.0.1:8080/api/v1/installations/123e4567-e89b-42d3-a456-426614174000 \
  -H 'Authorization: Bearer <TOKEN_DEV_LOCAL>'
```

No repitas el mismo UUID de ejemplo si ya lo registraste: la creación con ID existente en **la misma organización** es idempotente y puede devolver la instalación previa; si el UUID pertenece a otra organización responde 409. Trial sólo puede activarse una vez. Para emitir licencias DEV, además del token se requieren `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE=true` y una semilla Ed25519 local válida (`HIQNET_PLATFORM_SIGNING_KEY_B64`); por defecto está deshabilitado. Nunca uses una semilla de ejemplo para emitir fuera de pruebas.

## Pruebas y evolución del contrato

Usa exclusivamente una **base aislada** como `hiqnet_test_p00` de [desarrollo local](local.md) para `TEST_DATABASE_URL`. Nunca apuntes a la base `hiqnet` principal. `cargo test --workspace` aplica migraciones SQLx a esa base, ejercita creación, consultas, trial, emisión sintética, errores y limpieza de fixtures. Si el test falla antes de limpiar, elimina sólo fixtures sintéticos de la base aislada tras revisar sus UUID; no borres el volumen. Ejecuta `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `python3 scripts/check_repository.py` y build Docker.

Fuente canónica: DTO HTTP y anotaciones `utoipa` en `apps/api/src/contract.rs` y `apps/api/src/lib.rs`. `OpenApiRouter` registra handlers y obtiene operaciones OpenAPI desde las **mismas rutas**. `openapi/openapi.json` es un snapshot generado, no otro contrato manual. Tras cualquier cambio de endpoint, regenera en UTF-8:

```sh
cargo run --quiet -p hiqnet-platform-api --bin generate-openapi > openapi/openapi.json
openapi-spec-validator --schema 3.1 openapi/openapi.json
```

En Windows PowerShell 5, `>` puede alterar la codificación. Usa `python -c "import subprocess,pathlib; pathlib.Path('openapi/openapi.json').write_bytes(subprocess.check_output(['cargo','run','--quiet','-p','hiqnet-platform-api','--bin','generate-openapi']))"`. Instala `openapi-spec-validator==0.9.0` en un entorno Python aislado para validar. CI compara el JSON byte a byte con el generado, comprueba sintaxis OpenAPI 3.1 y ejecuta tests del router/snapshot, seguridad, Swagger y PostgreSQL. Cada endpoint nuevo debe aportar **en el mismo PR** ruta y anotación (`operation_id` estable), DTO/esquemas y ejemplos, respuestas y errores reales, seguridad aplicada, pruebas HTTP y documentación actualizada; no añadas rutas Axum fuera de `OpenApiRouter` salvo `/docs` y su documento opcionales.
