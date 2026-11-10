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

Rutas de identidad comercial (P02.2-A), independientes del token DEV anterior:

| Método | Ruta | Autenticación | Éxito | Casos frecuentes |
| --- | --- | --- | --- | --- |
| POST | `/api/v1/auth/login` | Credenciales en el cuerpo | 204 + cookie `hiqnet_session` | 401 credenciales/cuenta; 403 origen; 429 rate limit; 503 BD o `auth_overloaded` |
| POST | `/api/v1/auth/logout` | Cookie de sesión | 204 y cookie borrada | 403 origen; idempotente si no hay sesión |
| GET | `/api/v1/auth/me` | Cookie de sesión | 200 identidad | 401 sesión ausente/expirada/revocada; 503 BD |
| POST | `/api/v1/auth/password/change` | Cookie de sesión | 204 + nueva cookie | 400 política; 401 actual incorrecta/sesión; 403 origen; 409 concurrencia; 429 rate limit; 503 BD |
| POST | `/api/v1/auth/sessions/revoke` | Cookie de sesión | 204 y cookie borrada | 401 sesión; 403 origen; 503 BD |
| GET | `/api/v1/organizations` | Cookie de sesión | 200 `items` + `next_cursor` | 400 cursor/límite; 401 sesión; 503 BD |
| POST | `/api/v1/organizations` | Cookie de sesión | 201 + `Location` | 400 nombre; 401 sesión; 403 origen; 429 límite; 503 BD |
| GET | `/api/v1/organizations/{id}` | Cookie de sesión | 200 organización | 400 UUID; 401 sesión; 404 ajena; 503 BD |
| GET | `/api/v1/organizations/{id}/members` | Cookie de sesión + membresía | 200 `items` + `next_cursor` | 400 UUID/cursor; 401 sesión; 404 ajena; 503 BD |
| POST | `/api/v1/organizations/{id}/members` | Cookie de sesión + owner | 201 + `Location` | 400 rol; 401; 403 origen/rol; 404 ajena/cuenta; 409 duplicada; 503 BD |
| PATCH | `/api/v1/organizations/{id}/members/{account_id}` | Cookie de sesión + owner | 200 rol | 400 rol; 401; 403; 404; 409 último owner; 503 BD |
| DELETE | `/api/v1/organizations/{id}/members/{account_id}` | Cookie de sesión + owner | 204 | 401; 403; 404; 409 último owner; 503 BD |

La sesión comercial no es el token DEV ni una identidad POS. El `account_id` de `/me` proviene **solo** de la cookie verificada en PostgreSQL; ningún body, query o encabezado puede elegirlo. Cada login rota sesiones previas y persiste únicamente el hash SHA-256 del token. La cookie es `HttpOnly`, `SameSite=Lax`, `Path=/api` y `Secure` salvo en `HIQNET_PLATFORM_ENV=development` con `HIQNET_PLATFORM_COOKIE_SECURE=false` explícito; el arranque rechaza desactivar `Secure` fuera de development. **En producción toda mutación exige `Origin` exacto** que coincida con alguna entrada de `HIQNET_PLATFORM_ALLOWED_ORIGIN` (lista separada por comas, comparación textual sin comodines); `null`, valores múltiples, malformados o ausentes se rechazan. Sólo development admite ausencia de `Origin` para curl/Swagger; sin origen configurado se falla cerrado. La contraseña se protege con Argon2id (m=19456 KiB, t=2, p=1, salt aleatorio) y nunca se registra; `HIQNET_PLATFORM_AUTH_CONCURRENCY` (4 por defecto) acota verificaciones simultáneas y responde 503 `auth_overloaded` bajo saturación.

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

## Cuenta sintética y sesión comercial DEV

No existe registro público. Crea una cuenta **sólo local** con el comando de bootstrap; exige `HIQNET_PLATFORM_ENV=development` y `HIQNET_PLATFORM_DEV_BOOTSTRAP=true`, y lee la contraseña por entrada estándar (nunca por argumentos):

```sh
HIQNET_PLATFORM_ENV=development HIQNET_PLATFORM_DEV_BOOTSTRAP=true \
  cargo run -p hiqnet-platform-api --bin dev-bootstrap-account -- cuenta@example.test "Cuenta DEV"
# introduce una contraseña local de al menos 12 caracteres cuando se solicite
```

En PowerShell, define ambas variables con `$env:` y ejecuta el binario; no pegues la contraseña en el historial. Después:

```sh
curl -i -c cookies.txt -X POST http://127.0.0.1:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -H 'Origin: http://127.0.0.1:8080' \
  -d '{"email":"cuenta@example.test","password":"<CONTRASEÑA_DEV_LOCAL>"}'
# 204 y Set-Cookie: hiqnet_session=...; HttpOnly; Path=/api; SameSite=Lax
curl -i -b cookies.txt http://127.0.0.1:8080/api/v1/auth/me
# 200 {"account_id":"...","email":"cuenta@example.test"}
curl -i -b cookies.txt -X POST http://127.0.0.1:8080/api/v1/auth/logout \
  -H 'Origin: http://127.0.0.1:8080'
# 204 y cookie con Max-Age=0
```

### Organizaciones autenticadas

Con cookie comercial se pueden crear y consultar únicamente organizaciones propias. `GET /api/v1/organizations` devuelve las organizaciones activas con membresía activa, ordenadas por nombre e id. `POST /api/v1/organizations` acepta sólo `display_name`, exige Origin válido y asigna `owner` en la misma transacción que crea la organización, la membresía y las auditorías `organization_created`/`membership_added`; no es idempotente y no admite `account_id`, `owner_id`, `tenant_id` ni `role` del cliente. `GET /api/v1/organizations/{id}` responde 404 neutral para organizaciones ajenas, archivadas, `legacy_unclaimed` o sin membresía activa.

```sh
curl -i -b cookies.txt -X POST http://127.0.0.1:8080/api/v1/organizations \
  -H 'Content-Type: application/json' \
  -H 'Origin: http://127.0.0.1:8080' \
  -d '{"display_name":"Mi negocio"}'
# 201 y Location: /api/v1/organizations/<uuid>
curl -i -b cookies.txt 'http://127.0.0.1:8080/api/v1/organizations?limit=50'
```

La creación está limitada a 5 solicitudes por minuto y cuenta en esta instancia (protección técnica, no cuota de plan). Las organizaciones heredadas no se reclaman automáticamente.

### Paginación por cursor (P02.3-B)

`GET /api/v1/organizations` y `GET /api/v1/organizations/{id}/members` aceptan `limit` (1–100, por defecto 50) y `cursor`. El campo `next_cursor` de la respuesta es opaco: se envía tal cual como `cursor` para obtener la página siguiente y se omite en la última página. El orden es estable: organizaciones por `display_name` e `id`; miembros por `joined_at` y `account_id`. El cursor incluye versión y tipo de listado; el de miembros queda vinculado a la organización. Reutilizarlo en otro listado u organización, enviar una versión desconocida o superar 2048 caracteres devuelve 400. Un cursor malformado o un límite fuera de rango también devuelven 400. El límite es técnico y no representa una cuota comercial por plan. El formato de `items` es compatible con P02.3-A; sólo se añadió `next_cursor`.

### Membresías y roles

| Método | Ruta | Rol requerido | Éxito | Casos frecuentes |
| --- | --- | --- | --- | --- |
| GET | `/api/v1/organizations/{id}/members` | Cualquier miembro activo | 200 `items` | 400 UUID/cursor/límite; 401 sesión; 404 organización ajena o sin membresía; 503 BD |
| POST | `/api/v1/organizations/{id}/members` | `owner` | 201 + `Location` | 400 UUID/rol; 401 sesión; 403 origen o rol insuficiente; 404 organización ajena o cuenta inexistente/deshabilitada; 409 membresía previa; 503 BD |
| PATCH | `/api/v1/organizations/{id}/members/{account_id}` | `owner` | 200 rol actualizado | 400 UUID/rol; 401 sesión; 403 origen o rol insuficiente; 404 organización/membresía ajena; 409 rol sin cambios o último owner; 503 BD |
| DELETE | `/api/v1/organizations/{id}/members/{account_id}` | `owner` | 204 | 400 UUID; 401 sesión; 403 origen o rol insuficiente; 404 organización/membresía ajena; 409 último owner; 503 BD |

`POST` recibe `{"account_id":"<uuid>","role":"member"}`; el `account_id` identifica **al sujeto** de la operación, nunca al actor, que proviene sólo de la cookie. Los roles válidos son `owner`, `admin` y `member` (versión 1). La respuesta de listado expone únicamente `account_id`, `role` y `joined_at`: no incluye correo, nombre visible, credenciales ni sesiones. Owner, admin y member pueden consultar; sólo owner administra. Un admin o member que intenta mutar recibe 403; una organización ajena o inexistente devuelve 404 neutral. El último `owner` activo no puede degradarse ni revocarse (409). Cada alta, cambio de rol y revocación se confirma con su evento de auditoría en la misma transacción; si la auditoría falla, no queda cambio.

La reincorporación de una membresía revocada se rechaza con 409 mientras no exista una política aprobada, porque la clave primaria `(organization_id, account_id)` no admite una segunda fila. La revocación es persistente y el acceso cesa en la siguiente solicitud. La administración de invitaciones, correo y React Portal sigue fuera de alcance.

```sh
curl -i -b cookies.txt 'http://127.0.0.1:8080/api/v1/organizations/<ORG>/members?limit=50'
curl -i -b cookies.txt -X POST http://127.0.0.1:8080/api/v1/organizations/<ORG>/members \
  -H 'Content-Type: application/json' -H 'Origin: http://127.0.0.1:8080' \
  -d '{"account_id":"<CUENTA>","role":"member"}'
curl -i -b cookies.txt -X PATCH http://127.0.0.1:8080/api/v1/organizations/<ORG>/members/<CUENTA> \
  -H 'Content-Type: application/json' -H 'Origin: http://127.0.0.1:8080' \
  -d '{"role":"admin"}'
curl -i -b cookies.txt -X DELETE http://127.0.0.1:8080/api/v1/organizations/<ORG>/members/<CUENTA> \
  -H 'Origin: http://127.0.0.1:8080'
# 204; la cuenta revocada deja de ver la organización y sus miembros
```

### Cambio de contraseña y revocación de sesiones

La nueva contraseña debe tener 12–128 caracteres, al menos una letra y un dígito, sin caracteres de control y ser distinta de la actual. El cambio exige la contraseña actual y una sesión válida; en una sola transacción actualiza el hash y `password_changed_at`, revoca todas las sesiones anteriores, audita `password_changed` y emite una cookie nueva. Si otra operación cambió la credencial, responde 409 y no deja cambios parciales.

```sh
curl -i -b cookies.txt -X POST http://127.0.0.1:8080/api/v1/auth/password/change \
  -H 'Content-Type: application/json' \
  -H 'Origin: http://127.0.0.1:8080' \
  -d '{"current_password":"<ACTUAL>","new_password":"<NUEVA_12_128>"}'
# 204 y nueva cookie; la cookie anterior deja de servir
curl -i -b cookies.txt -X POST http://127.0.0.1:8080/api/v1/auth/sessions/revoke \
  -H 'Origin: http://127.0.0.1:8080'
# 204, todas las sesiones revocadas y cookie con Max-Age=0
```

Ambos endpoints aplican la misma política CSRF de producción y los límites de autenticación. No existe recuperación por correo ni registro público.

Swagger puede inspeccionar los esquemas; para probar con cookie usa curl o el navegador tras el login. Los ejemplos incluyen `Origin` porque en producción es obligatorio; en development puede omitirse para clientes locales, pero nunca en producción. Una sesión expira según `HIQNET_PLATFORM_SESSION_TTL_HOURS` (1–168, 8 por defecto), se revoca al hacer logout o al iniciar otro login, y deja de servir si la cuenta se deshabilita. El bloqueo por contraseña se activa a los 5 fallos durante 15 minutos; al vencer, el siguiente fallo inicia un ciclo nuevo sin reabrir el bloqueo. Además hay un límite en memoria de 10 intentos por minuto y correo, cota de 4096 claves y un semáforo de verificaciones Argon2id (`HIQNET_PLATFORM_AUTH_CONCURRENCY`); bajo saturación responde 503 `auth_overloaded`. No se confía en `X-Forwarded-For`: el límite por IP y el reparto multiinstancia requieren proxy confiable y almacenamiento compartido, pendientes. La medición de Argon2id es informativa: `cargo test -p platform-application benchmark_argon2id -- --ignored --nocapture`.

## Pruebas y evolución del contrato

Usa exclusivamente una **base aislada** como `hiqnet_test_p00` de [desarrollo local](local.md) para `TEST_DATABASE_URL`. Nunca apuntes a la base `hiqnet` principal. `cargo test --workspace` aplica migraciones SQLx a esa base, ejercita creación, consultas, trial, emisión sintética, errores y limpieza de fixtures. Si el test falla antes de limpiar, elimina sólo fixtures sintéticos de la base aislada tras revisar sus UUID; no borres el volumen. Ejecuta `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `python3 scripts/check_repository.py` y build Docker.

Fuente canónica: DTO HTTP y anotaciones `utoipa` en `apps/api/src/contract.rs` y `apps/api/src/lib.rs`. `OpenApiRouter` registra handlers y obtiene operaciones OpenAPI desde las **mismas rutas**. `openapi/openapi.json` es un snapshot generado, no otro contrato manual. Tras cualquier cambio de endpoint, regenera en UTF-8:

```sh
cargo run --quiet -p hiqnet-platform-api --bin generate-openapi > openapi/openapi.json
openapi-spec-validator --schema 3.1 openapi/openapi.json
```

En Windows PowerShell 5, `>` puede alterar la codificación. Usa `python -c "import subprocess,pathlib; pathlib.Path('openapi/openapi.json').write_bytes(subprocess.check_output(['cargo','run','--quiet','-p','hiqnet-platform-api','--bin','generate-openapi']))"`. Instala `openapi-spec-validator==0.9.0` en un entorno Python aislado para validar. CI compara el JSON byte a byte con el generado, comprueba sintaxis OpenAPI 3.1 y ejecuta tests del router/snapshot, seguridad, Swagger y PostgreSQL. Cada endpoint nuevo debe aportar **en el mismo PR** ruta y anotación (`operation_id` estable), DTO/esquemas y ejemplos, respuestas y errores reales, seguridad aplicada, pruebas HTTP y documentación actualizada; no añadas rutas Axum fuera de `OpenApiRouter` salvo `/docs` y su documento opcionales.


## Enrolamiento seguro de instalaciones (G4.2-B)

La sección **Enrollment** de Swagger expone el flujo device-code + aprobación owner + proof Ed25519. El POS inicia el challenge sin cookie comercial; el owner aprueba desde el navegador; el dispositivo firma el mensaje canónico exacto y Platform crea la instalación, la identidad y el binding.

| Método | Ruta | Seguridad | Resultado |
| --- | --- | --- | --- |
| POST | `/api/v1/enrollment-requests` | Pública + rate limit | 201 con secretos de entrega única |
| POST | `/api/v1/enrollment-requests/{user_code}/approve` | Cookie comercial + `Origin` + owner | 200 |
| POST | `/api/v1/enrollment-requests/{device_code}/proof` | Firma Ed25519 | 201 |
| GET | `/api/v1/organizations/{id}/installations` | Cookie comercial + cualquier membresía | 200 |

La cabecera `Idempotency-Key` del inicio es opcional y tiene un máximo de 128 caracteres ASCII visibles. Si se repite con el mismo payload después de entregar secretos, la respuesta es `409 enrollment_restart_required`; con payload distinto es `409 idempotency_conflict`. Los códigos, el nonce y las claves privadas no se persisten ni se devuelven en endpoints posteriores.

Si entre `/approve` y `/proof` la organización se archiva, la cuenta se deshabilita, la membresía se revoca o el actor deja de ser owner, `/proof` devuelve `409 enrollment_invalid`. `/approve` mantiene `403 forbidden` para un rol insuficiente conocido en ese momento; no se mezclan ambas semánticas.

Mensaje firmado exacto (UTF-8, sin JSON):

```text
hiqnet-enrollment-proof:v1
request=<device_code>
installation=<installation_id>
key=<public_key>
nonce=<nonce>
```

TTL por defecto: 600 segundos (`HIQNET_PLATFORM_ENROLLMENT_TTL_SECONDS`, 60–3600). Los rate limits de enrolamiento son independientes del login y de organizaciones y siguen siendo memoria local por instancia: begin y proof se limitan por peer real de la conexión TCP (`ConnectInfo`, no `X-Forwarded-For`) y approve por `account_id`. Variables: `HIQNET_PLATFORM_ENROLLMENT_BEGIN_RATE_LIMIT` (30), `HIQNET_PLATFORM_ENROLLMENT_PROOF_RATE_LIMIT` (60), `HIQNET_PLATFORM_ENROLLMENT_APPROVE_RATE_LIMIT` (30) y `HIQNET_PLATFORM_ENROLLMENT_RATE_WINDOW_SECONDS` (60). No son una cuota comercial. Multi-instancia requerirá un limitador distribuido en G4.2-E.

Prueba real en local (sustituye los valores por los sintéticos de tu ejecución; no publiques secretos):

```sh
curl -sS -X POST http://127.0.0.1:8080/api/v1/enrollment-requests \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: prueba-local-1' \
  -d '{"installation_id":"<UUID>","public_key":"ed25519:v1:<BASE64URL>","device_label":"Caja local","app_version":"0.1.0","platform":"linux"}'
```

El mensaje canónico se firma con la clave privada local; Platform sólo almacena la clave pública, el fingerprint SHA-256 y hashes de los códigos/nonce. Los secretos no aparecen en Swagger, auditoría ni logs.
