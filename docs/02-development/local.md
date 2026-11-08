# Desarrollo local de Platform

## Preparación

Windows 11 (PowerShell + Docker Desktop con motor Linux), o Linux/Omarchy con Docker Engine/Compose; Git y Rust mediante `rustup`. La versión y componentes de Rust se fijan en `rust-toolchain.toml`. En Linux, permisos de Docker según la política de tu equipo. **Linux/Omarchy no se ha verificado en host real para P00.**

Comprueba `docker version`, `docker compose version`, `cargo --version` y que los puertos 5432 y 8080 estén libres (`Get-NetTCPConnection` en PowerShell; `ss -ltn` en Linux). El volumen Compose `postgres-data` es persistente. No ejecutes `docker compose down -v`.

1. Copia `.env.example` a `.env` (`Copy-Item .env.example .env` o `cp .env.example .env`). `.env` está ignorado por Git.
2. Sustituye `POSTGRES_PASSWORD` por una contraseña **aleatoria local de desarrollo** y el mismo valor dentro de `DATABASE_URL`. No uses el literal del ejemplo ni compartas `.env`. Si el volumen **ya está inicializado**, cambiar sólo `.env` no cambia la contraseña de PostgreSQL: conserva credenciales existentes o rota el rol mediante `ALTER ROLE` en sesión local antes de cambiar `.env`; haz backup primero si contiene datos valiosos. No publiques la contraseña en terminal, PR ni logs.
3. Deja `HIQNET_PLATFORM_ADMIN_TOKEN` vacío para deshabilitar mutaciones administrativas. La emisión DEV está deshabilitada por defecto. No se requiere clave de firma para el arranque.
4. `docker compose config --quiet` valida configuración sin imprimir secretos; `docker compose up -d postgres` inicia la BD; `docker compose ps` debe mostrar `healthy`. Ambas publicaciones de puerto (API y PostgreSQL) escuchan sólo en `127.0.0.1` del host.

## API, consultas y esquema

`docker compose up --build -d api` compila/inicia la API y aplica migraciones SQLx de `migrations/` en `open()`. Comprueba `docker compose logs --tail=50 api` (no compartas logs con datos privados), `curl http://127.0.0.1:8080/health` y `curl http://127.0.0.1:8080/ready`. `/health` devuelve 200 si responde el proceso; `/ready` ejecuta `SELECT 1`, devuelve 200 con BD o 503 sin ella. Si PostgreSQL falla **al arranque**, la API termina en lugar de declarar readiness.

También puedes iniciar `cargo run -p hiqnet-platform-api` desde el host usando `DATABASE_URL` de `.env`, siempre que el puerto 8080 esté libre (detén el servicio `api` de Compose antes). La API no arranca sin `DATABASE_URL`. Usa `docker compose exec postgres psql -U hiqnet -d hiqnet` para consultar; ajusta usuario/BD a `.env`. Inspecciona `\dt`, `SELECT version, success FROM _sqlx_migrations;` y `SELECT table_name FROM information_schema.tables WHERE table_schema='public';`.

La migración `0001_init.sql` crea `installation` (ID, organización UUID, etiqueta única por organización, estado, fechas y trial), `issued_license` (grant, FK a instalación, organización, plan, JSONB de features, validez, payload y firma) y sus índices. `_sqlx_migrations` registra la versión aplicada. No hay tablas de clientes, facturación ni sync; no cambies una migración publicada.

Consulta [API y Swagger (P01)](api.md) para abrir la documentación, probar rutas y actualizar el contrato.

El esquema comercial P02.1 y sus relaciones se documentan en [identidad comercial](../01-architecture/commercial-identity.md). P02.1 no publica rutas comerciales ni altera tu base principal durante las pruebas de migración.

## Pruebas aisladas

Crea una BD nueva sin reutilizar la de desarrollo: `docker compose exec postgres psql -U hiqnet -d postgres -c 'CREATE DATABASE hiqnet_test_p00;'`. Configura `TEST_DATABASE_URL` con **la misma contraseña de `.env`** y nombre `hiqnet_test_p00` (sin imprimirla); en PowerShell, por ejemplo: `$env:TEST_DATABASE_URL = ((Get-Content .env | Where-Object { $_ -match '^DATABASE_URL=' }).Substring(13) -replace '/hiqnet$', '/hiqnet_test_p00')`. En Bash: carga `.env` en shell local sólo si confías en ese archivo y sustituye el nombre de BD al exportar `TEST_DATABASE_URL`. Nunca apuntes `TEST_DATABASE_URL` a tu base de desarrollo o producción. Los tests aplican migraciones desde cero, prueban operaciones SQL sintéticas de ambas tablas y revierten la fixture en una transacción.

Ejecuta `cargo build --workspace`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `python3 -m unittest discover -s scripts -p 'test_*.py'` y `python3 scripts/check_repository.py`. El gate de archivos comprueba sólo rutas versionadas y encabezados PEM: **no sustituye secret scanning completo**. CI crea su propia base `hiqnet_test` sobre un servicio PostgreSQL efímero, ejecuta migraciones en tests y nunca toca tu volumen local.

Para P02.2-A usa bases nuevas y aisladas (por ejemplo `hiqnet_test_p022` y `hiqnet_upgrade_test_p022`): la migración 0003 añade credenciales, sesiones y auditoría de autenticación. Los tests cubren Argon2id, bloqueo, rotación, expiración, revocación, CSRF/Origin, rate limit y migración desde 0002. Para P02.1, usa una `TEST_DATABASE_URL` **nueva** e aislada como `hiqnet_test_p021` y crea otra base distinta, por ejemplo `docker compose exec postgres psql -U hiqnet -d postgres -c 'CREATE DATABASE hiqnet_upgrade_test_p021;'`. Configura `TEST_UPGRADE_DATABASE_URL` con esta última usando el mismo método de sustitución que `TEST_DATABASE_URL`; en PowerShell: `$env:TEST_UPGRADE_DATABASE_URL = ((Get-Content .env | Where-Object { $_ -match '^DATABASE_URL=' }).Substring(13) -replace '/hiqnet$', '/hiqnet_upgrade_test_p021')`. Crea además una tercera base aislada para el upgrade de autenticación y configura `TEST_AUTH_UPGRADE_DATABASE_URL`; evita que las pruebas de migración compartan base y compitan entre sí. No uses la base principal ni una base con datos valiosos: el test de upgrade arranca sólo con P00/P01, inserta instalaciones/licencias sintéticas y aplica P02.1. La base `TEST_DATABASE_URL` prueba migración desde cero, índices y restricciones; los tests de cuentas dejan eventos sintéticos por diseño de auditoría inmutable. CI destruye sus bases efímeras al terminar; las bases locales sólo se eliminan manualmente tras verificar que no contienen datos ajenos. En P02.1 el API DEV exige organización preexistente; véase [ejemplo local](api.md).

## Backup y restore sintético (sin sobrescribir bases)

Usa sólo la BD de pruebas, después de insertar datos sintéticos. Estos comandos evitan redirigir binarios mediante Windows PowerShell 5, que puede corromperlos:

```text
docker compose exec -T postgres pg_dump -U hiqnet -d hiqnet_test_p00 -Fc -f /tmp/hiqnet_p00.dump
docker cp hiqnet-platform-postgres:/tmp/hiqnet_p00.dump .dev-data/hiqnet_p00.dump
docker compose exec postgres psql -U hiqnet -d postgres -c 'CREATE DATABASE hiqnet_restore_p00;'
docker cp .dev-data/hiqnet_p00.dump hiqnet-platform-postgres:/tmp/hiqnet_restore_p00.dump
docker compose exec -T postgres pg_restore --exit-on-error -U hiqnet -d hiqnet_restore_p00 /tmp/hiqnet_restore_p00.dump
docker compose exec postgres psql -U hiqnet -d hiqnet_restore_p00 -c 'SELECT count(*) FROM installation;'
```

Crea `.dev-data` manualmente antes de `docker cp` (`New-Item -ItemType Directory -Force .dev-data` en PowerShell o `mkdir -p .dev-data` en Bash). Está ignorado por Git. Cambia los nombres si ya existen; **no uses** `--clean` ni borres una base existente. Dumps pueden incluir datos privados: nunca los subas a Git ni los compartas; backups reales requieren política de cifrado/retención aparte. Para recuperar una BD dañada, detén la API, conserva backup y volumen, crea una BD nueva, restaura allí, verifica integridad/conteos y sólo entonces ajusta la URL en `.env` con autorización.

## Parada y diagnósticos

`docker compose stop api postgres` detiene sin borrar datos. Reinicia con `docker compose up -d postgres` y espera `healthy` antes de `docker compose up -d api`. `docker compose ps` y `docker compose logs --tail=50 postgres api` diagnostican fallos. Si falla conexión, comprueba Docker Desktop/Engine, puertos, URL y contraseña (volumen previo no acepta contraseña nueva de `.env` sin rotación). Si falla migración, consulta `_sqlx_migrations` y logs sin modificar `0001_init.sql`. Si `/health` funciona pero `/ready` devuelve 503, revisa el servicio PostgreSQL; si la API termina al iniciar, revisa `DATABASE_URL`/migraciones. No uses `down -v` para resolver fallos.

## Release y gobierno

El workflow `release-platform.yml` sólo publica tras un **push de tag válido** `platform-v<SemVer>` correspondiente a versión del workspace (sin metadata `+build`, incompatible con tags de imagen Docker). `workflow_dispatch` valida/build sin publicar (dry-run); ejecutar manualmente en `main` no crea release. Protección de `main`/tags y GitHub secret scanning requieren autorización administrativa; CI sola no impide push directo ni creación de tags maliciosos por quien tenga permisos. No se publicó tag ni release en P00.


## Levantar Platform local (G4.2-B)

1. Copia `.env.example` a `.env` y define una contraseña local; no publiques `.env`.
2. `docker compose up -d postgres`
3. `docker compose up --build -d api` (o `cargo run -p hiqnet-platform-api --bin hiqnet-platform-api` con el puerto libre).
4. Swagger: <http://127.0.0.1:8080/docs/> y OpenAPI: <http://127.0.0.1:8080/api-docs/openapi.json>.
5. PostgreSQL local: `127.0.0.1:5432`, usuario/BD de `.env`.
6. UI de base de datos opcional: `docker compose --profile db-ui up -d adminer` y abre <http://127.0.0.1:8081/>. Usa `postgres` como servidor, y el usuario, contraseña y base de tu `.env`/entorno de pruebas. Está bindeada sólo a `127.0.0.1` y no forma parte de la imagen ni del despliegue de la API.
7. Apagado: `docker compose --profile db-ui down` (o `docker compose stop adminer` para conservar el resto).

El flujo sintético completo (`login → organización → begin enrollment → approve → proof firmado → list installations`) se documenta en [API y Swagger](api.md#enrolamiento-seguro-de-instalaciones-g42b). Para el E2E de este incremento se usó una base aislada `hiqnet_g42b`; no se modifican datos de otras bases.
