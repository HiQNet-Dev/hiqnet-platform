# P02.2 — Autenticación comercial: diseño aprobado, implementación P02.2-A y pendientes

Documento de seguimiento de la estrategia definida en [ADR-0002](adr-0002-authentication-strategy.md). Distingue explícitamente lo aprobado, lo implementado en P02.2-A y lo que queda para P02.2-B o posteriores.

## Diseño aprobado

Autenticación propia con Argon2id y sesiones persistidas en PostgreSQL; OIDC queda como opción futura, no como dependencia inicial. Principios: contraseñas sólo como hash resistente a ataques offline, tokens de sesión opacos y nunca recuperables desde persistencia, revocación real, mensajes neutrales, actor derivado exclusivamente de la sesión verificada y separación estricta respecto de usuarios locales del POS.

## Implementado en P02.2-A y P02.2-B

- `migrations/0003_commercial_auth.sql` crea `commercial_credential`, `commercial_session` y `commercial_auth_event`. No modifica 0001 ni 0002 ni usa cascadas destructivas.
- API: `POST /api/v1/auth/login`, `POST /api/v1/auth/logout` y `GET /api/v1/auth/me`, con DTO, OpenAPI 3.1 y Swagger.
- Argon2id (m=19456 KiB, t=2, p=1) con salt aleatorio, verificación en `spawn_blocking` y hash señuelo para igualar tiempos ante cuentas inexistentes.
- Token de sesión de 32 bytes; sólo SHA-256 se guarda. Rotación de sesiones en cada login, expiración configurable y revocación real.
- Cookie `HttpOnly`, `SameSite=Lax`, `Path=/api` y `Secure` salvo development explícito. El arranque rechaza desactivar `Secure` fuera de development.
- CSRF: en producción `Origin` obligatorio y exacto; `null`, múltiples, malformados o ausentes se rechazan. Sólo development admite clientes sin `Origin` para curl/Swagger. Sin origen configurado se falla cerrado. SameSite es defensa adicional.
- Bloqueo de 5 fallos durante 15 minutos; al vencer, el contador se reinicia atómicamente y un fallo posterior no reabre el bloqueo.
- Rate limit en memoria por correo (10/min), cota de 4096 claves que conserva bloqueos y semáforo global de Argon2id (`HIQNET_PLATFORM_AUTH_CONCURRENCY`) con 503 `auth_overloaded`.
- El extractor de sesión responde 401 ante sesión inválida y 503 neutral ante indisponibilidad de PostgreSQL, sin filtrar detalles internos.
- Bootstrap DEV sin registro público: exige `HIQNET_PLATFORM_ENV=development` y `HIQNET_PLATFORM_DEV_BOOTSTRAP=true`, y lee la contraseña por entrada estándar.

P02.2-B añade `POST /api/v1/auth/password/change` y `POST /api/v1/auth/sessions/revoke`, sin migración nueva: reutiliza `commercial_credential`, `commercial_session` y `commercial_auth_event`. El cambio de contraseña exige sesión y contraseña actual, aplica la política (12–128 caracteres, al menos una letra y un dígito, sin controles, distinta de la actual), verifica Argon2id, y en **una única transacción** actualiza hash y `password_changed_at`, revoca todas las sesiones, registra `password_changed` y crea la nueva sesión. La comparación optimista del hash impide que dos cambios concurrentes ganen; el perdedor recibe 409. La revocación total invalida todas las sesiones (incluida la actual), audita `session_revoked` y borra la cookie. Ambos endpoints exigen Origin válido y comparten el limitador y el semáforo Argon2id.

La medición de Argon2id es informativa y reproducible, no un gate de CI: `cargo test -p platform-application benchmark_argon2id -- --ignored --nocapture` (iteraciones con `ARGON2_BENCH_ITERATIONS`). En este equipo Windows con build debug el promedio hash+verify medido fue ≈0,85 s (≈1,4 s bajo carga concurrente); release es menor.

## Pendiente para P02.2-B y posteriores

- Verificación de correo y activación de cuentas productivas.
- Recuperación de contraseña por correo con tokens de un solo uso y expiración corta.
- Listado de organizaciones del actor y sesiones simultáneas por dispositivo.
- MFA, gestión de dispositivos y endurecimiento frente a phishing.
- Rate limiting por IP con proxy explícitamente confiable y limitador compartido entre instancias (sin Redis en esta fase).
- Portal React y API multitenant amplia de P02.3.

## Diccionario 0003 (implementado)

| Tabla | Columna | Tipo | Nulo | Restricción |
| --- | --- | --- | --- | --- |
| `commercial_credential` | `account_id` | UUID | No | PK y FK a `commercial_account` RESTRICT |
| | `password_hash` | TEXT | No | PHC Argon2id; nunca contraseña |
| | `password_changed_at` | TIMESTAMPTZ | No | fecha del hash |
| | `failed_attempts` | INTEGER | No | `>= 0`, default 0 |
| | `locked_until` | TIMESTAMPTZ | Sí | `>= created_at` si existe |
| | `created_at` | TIMESTAMPTZ | No | alta |
| `commercial_session` | `id` | UUID | No | PK |
| | `account_id` | UUID | No | FK RESTRICT |
| | `token_hash` | BYTEA | No | UNIQUE; SHA-256 del token |
| | `created_at` / `expires_at` | TIMESTAMPTZ | No | `expires_at > created_at` |
| | `revoked_at` | TIMESTAMPTZ | Sí | `>= created_at` |
| | `last_seen_at` | TIMESTAMPTZ | No | metadato mínimo |
| | `user_agent_hash`, `ip_hash` | BYTEA | Sí | reservados; no se pueblan aún |
| `commercial_auth_event` | `id` | UUID | No | PK |
| | `account_id` | UUID | Sí | FK RESTRICT |
| | `action` | TEXT | No | CHECK: login_success/failure, logout, session_revoked, account_locked, password_changed |
| | `occurred_at` | TIMESTAMPTZ | No | fecha del evento |
| | `ip_hash` | BYTEA | Sí | reservado |
| | `metadata` | JSONB | No | `{}` por defecto; sin secretos |

Índices: sesiones activas por cuenta, expiración, eventos por cuenta/acción. Trigger append-only en `commercial_auth_event`. No hay cascadas destructivas.

## Riesgos residuales

- MFA, phishing y gestión de dispositivos quedan fuera.
- Recuperación y verificación de correo requieren servicio y diseño adicional.
- El rate limit por IP y multiinstancia requiere proxy confiable y almacenamiento compartido; el actual es por correo, por instancia y con protección global de concurrencia.
- La revocación es inmediata porque cada solicitud consulta PostgreSQL; no hay caché de sesión.
- No declarar autenticación lista para producción sin completar los puntos anteriores y P02.3.
