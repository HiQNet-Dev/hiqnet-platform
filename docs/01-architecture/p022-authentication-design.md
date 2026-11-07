# P02.2 — Autenticación comercial: diseño e implementación P02.2-A

Este documento registra el diseño aprobado para la siguiente implementación. **No se añade código, migración ni endpoint en este paso.** La estrategia se define en [ADR-0002](adr-0002-authentication-strategy.md).

## Estrategia elegida

Autenticación propia con Argon2id y sesiones persistidas en PostgreSQL. OIDC queda como opción futura, no como dependencia inicial.

## Migraciones previstas (aún no creadas)

- `0003_commercial_auth.sql`:
  - `commercial_credential`: `account_id` PK/FK, `password_hash` TEXT Argon2id, `password_changed_at`, `failed_attempts`, `locked_until`.
  - `commercial_session`: `id` PK, `account_id` FK, `token_hash` UNIQUE, `created_at`, `expires_at`, `revoked_at`, `last_seen_at`, `user_agent_hash` opcional, `ip_hash` opcional.
  - `commercial_auth_event`: auditoría append-only para login, logout, bloqueo, cambio de contraseña y revocación.
- No modificar `0001` ni `0002`.
- Sin borrado en cascada de auditoría.

## Endpoints previstos

- `POST /api/v1/auth/register` (posible, sólo si se permite alta inicial; o creación por administración).
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/me`
- `POST /api/v1/auth/password/change`
- `POST /api/v1/auth/sessions/revoke`
- `GET /api/v1/organizations` (organizaciones del actor)

Todos con DTO Rust, OpenAPI 3.1, Swagger, errores normalizados y pruebas. Cookies para navegador y Bearer para API si se soporta.

## Seguridad mínima

- Argon2id con parámetros documentados.
- Mensajes neutrales para evitar enumeración.
- Rate limiting por IP y cuenta.
- Bloqueo temporal tras intentos fallidos.
- Cookies `HttpOnly`, `Secure`, `SameSite=Lax`/`Strict`.
- CSRF para mutaciones basadas en cookie.
- Rotación de sesión en login y cambio de contraseña.
- Revocación inmediata.
- Actor derivado exclusivamente de sesión.
- Organización activa validada contra membresía.
- Sin contraseñas, tokens o claves en logs.

## Pruebas requeridas

- Login correcto/incorrecto.
- Cuenta deshabilitada.
- Sesión expirada/revocada.
- Logout real.
- Rotación de sesión.
- Aislamiento entre organizaciones.
- Rate limiting.
- Cookies seguras.
- Migración desde P02.1.

## Implementado en P02.2-A

`migrations/0003_commercial_auth.sql` crea `commercial_credential` (PK/FK a cuenta, hash Argon2id, intentos y bloqueo), `commercial_session` (UUID, hash SHA-256 del token único, expiración, revocación, `last_seen_at`, metadatos nulos) y `commercial_auth_event` append-only. La API expone login, logout y `/me`; login rota sesiones y persiste sólo el hash. La cookie es HttpOnly/SameSite=Lax/Path=/api y Secure salvo DEV explícito. CSRF se cubre validando `Origin` en mutaciones y exigiendo un origen configurado; el rate limit por correo es en memoria (10/min) y el bloqueo de cuenta es 5 fallos/15 min. La verificación de contraseña corre en `spawn_blocking`. Parámetros Argon2id medidos en este equipo Windows con build de pruebas: m=19456 KiB, t=2, p=1; promedio local hash+verify ≈1,4 s en debug (`cargo test -p platform-application costo_argon2 -- --nocapture`), menor en release. Es un costo deliberado para dificultar ataques offline sin bloquear el runtime.

## Diccionario 0003

| Tabla | Columna | Tipo | Nulo | Restricción |
+| --- | --- | --- | --- | --- |
+| `commercial_credential` | `account_id` | UUID | No | PK y FK a `commercial_account` RESTRICT |
+| | `password_hash` | TEXT | No | PHC Argon2id; nunca contraseña |
+| | `password_changed_at` | TIMESTAMPTZ | No | fecha del hash |
+| | `failed_attempts` | INTEGER | No | `>= 0`, default 0 |
+| | `locked_until` | TIMESTAMPTZ | Sí | `>= created_at` si existe |
+| | `created_at` | TIMESTAMPTZ | No | alta |
+| `commercial_session` | `id` | UUID | No | PK |
+| | `account_id` | UUID | No | FK RESTRICT |
+| | `token_hash` | BYTEA | No | UNIQUE; SHA-256 del token |
+| | `created_at` / `expires_at` | TIMESTAMPTZ | No | `expires_at > created_at` |
+| | `revoked_at` | TIMESTAMPTZ | Sí | `>= created_at` |
+| | `last_seen_at` | TIMESTAMPTZ | No | metadato mínimo |
+| | `user_agent_hash`, `ip_hash` | BYTEA | Sí | reservados; no se pueblan aún |
+| `commercial_auth_event` | `id` | UUID | No | PK |
+| | `account_id` | UUID | Sí | FK RESTRICT |
+| | `action` | TEXT | No | CHECK: login_success/failure, logout, session_revoked, account_locked, password_changed |
+| | `occurred_at` | TIMESTAMPTZ | No | fecha del evento |
+| | `ip_hash` | BYTEA | Sí | reservado |
+| | `metadata` | JSONB | No | `{}` por defecto; sin secretos |
+
+Índices: sesiones activas por cuenta, expiración, eventos por cuenta/acción. Trigger append-only en `commercial_auth_event`. No hay cascadas destructivas.

## Riesgos pendientes

- MFA, phishing y gestión de dispositivos quedan fuera.
- Recuperación y verificación de correo requieren servicio y diseño adicional.
- El rate limit por IP requiere proxy confiable; el actual es por correo y por instancia.
- La revocación es inmediata porque cada solicitud consulta PostgreSQL; no hay caché de sesión.
- No declarar autenticación lista para producción sin completar los puntos anteriores y P02.3.
"