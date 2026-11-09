# ADR-0005 — Identidad y autenticación de personal HiQNet (staff)

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Fase:** C0 — Architecture Foundation
**Relacionado:** [ADR-0002](adr-0002-authentication-strategy.md), [ADR-0004](adr-0004-control-center-boundaries.md)

## 1. Contexto

`commercial_account` representa a un cliente de una organización. `HIQNET_PLATFORM_ADMIN_TOKEN` (`devAdminToken`) es una protección operativa de desarrollo, no una identidad. Ninguna de las dos puede ser la identidad del personal interno de HiQNet.

## 2. Problema

Necesitamos una identidad interna que:

- se autentique con credenciales propias;
- tenga roles y permisos internos;
- permita sesiones revocables y auditadas;
- no herede membresías comerciales;
- no pueda confundirse con una cuenta cliente;
- soporte MFA/WebAuthn/OIDC en el futuro sin rediseño.

## 3. Decisión propuesta

Crear un **dominio de identidad staff separado**, reutilizando la infraestructura técnica segura de ADR-0002 (Argon2id, sesiones persistidas, cookies HttpOnly, CSRF por `Origin`), pero con entidades, cookie, esquema OpenAPI y auditoría propios.

### Modelo de datos propuesto (sin DDL en esta fase)

| Entidad                 | Campos conceptuales                                                                                                                                                           |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `staff_account`         | `id`, `email`, `display_name`, `status`, `created_at`, `updated_at`                                                                                                           |
| `staff_credential`      | `account_id` (PK/FK), `password_hash`, `password_changed_at`, `failed_attempts`, `locked_until`, `created_at`                                                                 |
| `staff_session`         | `id`, `account_id`, `token_hash`, `created_at`, `expires_at`, `revoked_at`, `last_seen_at`                                                                                    |
| `staff_role_assignment` | `staff_account_id`, `role`, `granted_by`, `granted_at`, `revoked_at`                                                                                                          |
| `staff_audit_event`     | `id`, `staff_account_id` (nullable para eventos de sistema), `action`, `resource_type`, `resource_id`, `organization_id?`, `product_id?`, `result`, `occurred_at`, `metadata` |
| `staff_auth_event`      | `id`, `account_id?`, `action`, `occurred_at`, `ip_hash?`, `metadata`                                                                                                          |

### Convenciones

- Correo canónico: `email = lower(btrim(email))` con CHECK, como en `commercial_account`.
- Contraseña: sólo Argon2id en PHC; nunca texto plano ni en logs.
- Token de sesión: opaco de 32 bytes; sólo SHA-256 persistido.
- Cookie: `hiqnet_staff_session`, `HttpOnly`, `SameSite=Strict`, `Path=/api/v1/control`, `Secure` salvo development explícito.
- Security scheme OpenAPI: `staffSession` (distinto de `commercialSession` y `devAdminToken`).
- Actor derivado **exclusivamente** de la sesión verificada dentro de la transacción de cada operación.
- Bloqueo tras N fallos, rate limiting por correo y por IP confiable, mensajes neutrales.
- `staff_account.status` ∈ `active | disabled`. Deshabilitar revoca sesiones en la misma transacción.

### Endpoints propuestos (fase C1)

```
POST /api/v1/control/auth/login    → 204 + cookie
POST /api/v1/control/auth/logout   → 204
GET  /api/v1/control/auth/me       → 200 StaffSessionView
```

No se expone registro público. La creación de staff es una operación administrativa auditada (o bootstrap DEV con las mismas salvaguardas que P02.2).

## 4. Seguridad

- `Origin` exacto obligatorio en producción; sin origen configurado se falla cerrado.
- Nunca se acepta `commercialSession` como autoridad staff ni viceversa.
- `devAdminToken` no puede iniciar sesión staff.
- MFA/WebAuthn/OIDC quedan como evolución; el esquema de identidad debe permitir añadir `staff_external_identity`.
- Las sesiones staff se validan contra PostgreSQL en cada solicitud (sin caché de autorización), igual que la sesión comercial.

## 5. Consecuencias

- Migración `0005` dedicada (tras aprobación) sin tocar 0001–0004.
- Nuevo crate `platform-staff` (o módulo en `platform-application` + `platform-storage-postgres`) y handlers staff.
- Cookies y dominios separados del Customer Portal.
- Auditoría staff append-only (trigger como en `commercial_auth_event`).

## 6. Decisiones ratificadas

- **Modelo multirol-capaz:** `staff_role_assignment` admite varias filas; C1 usa un rol activo por cuenta.
- **Alta de staff:** bootstrap DEV (sólo `HIQNET_PLATFORM_ENV=development` + `HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP=true`, contraseña por stdin); creación normal como operación administrativa auditada.
- **Expiración de sesión:** 8 h absolutas, rotación en cada login, revocación inmediata.
- **Auditoría separada:** `staff_auth_event` (autenticación) y `staff_audit_event` (mutaciones administrativas) permanecen distintas.

## 7. Riesgos residuales

- MFA y passkeys no implementados en C1; se mitiga con sesión corta, bloqueo y auditoría.
- Recuperación de contraseña staff pendiente de diseño.
- El bootstrap DEV no debe habilitarse fuera de `development`.
