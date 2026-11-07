# P02.2 — Diseño de autenticación comercial (sin implementación)

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

## Riesgos pendientes

- MFA, phishing y gestión de dispositivos quedan fuera.
- Recuperación de contraseña requiere diseño adicional.
- La revocación inmediata necesita validar sesión en cada solicitud.
"