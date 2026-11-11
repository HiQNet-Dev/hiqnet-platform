# Transición de HiQNet Platform a HiQNet POS

**Estado:** Platform Beta cerrada. Este documento define la frontera y el trabajo que continúa en POS.
**Fecha:** 2026-11-09
**Relacionado:** [Notas de Beta](platform-beta-notes.md) · [Arquitectura](../01-architecture/platform.md) · [ADR-0011](../01-architecture/adr-0011-commercial-domain.md) · [ADR-0009](../01-architecture/adr-0009-pos-installation-control.md) · [Protocolo de enrolamiento](../01-architecture/g42-enrollment-protocol.md)

## 1. Regla de transición

Con la Beta cerrada, **Platform deja de recibir funcionalidad general**. A partir de aquí sólo se aceptan:

- Parches de seguridad.
- Corrección de defectos.
- Ajustes estrictamente necesarios para integrar POS.
- Correcciones de accesibilidad, integridad o compatibilidad.

Cualquier capacidad nueva se justifica por una necesidad de POS, no por completar la plataforma.

## 2. Arquitectura final de Platform

```text
Control Center (React)      Customer Portal (React)
        │                           │
        └────────── /api/* ─────────┘
                     │
              Platform API (Axum)
                     │
              platform-application
                     │
                platform-core
                     │
        platform-storage-postgres (SQLx)
                     │
                PostgreSQL 17
```

- Monolito modular; sin microservicios ni microfrontends.
- Identidades separadas: staff, comercial y dispositivo.
- Autorización en backend; la UI sólo refleja permisos.
- Auditoría append-only y transaccional con las mutaciones.
- Contratos OpenAPI 3.1 versionados en `openapi/openapi.json`.

## 3. Qué ofrece Platform a POS

### 3.1 Identidad y sesión comercial

- `POST /api/v1/auth/login`, `/logout`, `GET /me`, `POST /password/change`, `POST /sessions/revoke`.
- Cookie `hiqnet_session`: `HttpOnly`, `SameSite=Lax`, `Path=/api`, `Secure` en producción.
- Una sesión comercial **no** concede privilegios staff. POS no debe reutilizar esta sesión como identidad local.

### 3.2 Organizaciones y membresías

- `GET/POST /api/v1/organizations`, `GET /api/v1/organizations/{id}`.
- `GET/POST /api/v1/organizations/{id}/members`, `PATCH/DELETE /api/v1/organizations/{id}/members/{account_id}`.
- Aislamiento por organización con 404 neutral.

### 3.3 Enrolamiento de instalaciones

- `POST /api/v1/enrollment-requests` (begin).
- `POST /api/v1/enrollment-requests/{user_code}/approve` (aprobación por el owner comercial en el Portal).
- `POST /api/v1/enrollment-requests/{device_code}/proof` (proof firmado por la identidad del dispositivo).
- `POST /api/v1/installations`, `GET /api/v1/installations/{id}`, `GET /api/v1/organizations/{id}/installations`.

Puntos que POS debe respetar:

- La instalación es una **identidad de dispositivo**, no un usuario.
- El código de usuario expira; el código de dispositivo y el proof son de un solo uso.
- El staff no sustituye la autorización del owner comercial.
- `rehost` cambia la identidad criptográfica de la instalación y exige doble autoridad.

### 3.4 Organización ↔ producto

- `GET /api/v1/control/organizations/{id}/products` (staff).
- `POST` y `PATCH .../{product_id}` con `organizations.manage`.
- Estados `active` / `inactive`; sin borrado físico.
- Una asociación activa **no** implica una suscripción pagada.

### 3.5 Planes, suscripciones y entitlements

- `GET /api/v1/control/plans`, `GET/POST/PATCH` según capability.
- `GET/POST /api/v1/control/subscriptions` y transiciones `suspend`, `reactivate`, `cancel`, `renew`.
- `GET /api/v1/control/organizations/{id}/entitlements?product_id=...`.
- Orden de resolución: `organization_inactive` → `product_not_enabled` → `no_subscription` → `terminated` → `expired` → `not_started` → `suspended` → `active`.
- La expiración es lógica: el `status` persistido puede seguir `active` y el resolver devuelve `expired`.
- Trial único por `(organization_id, product_id)`.

Para POS, la consecuencia práctica es: **los derechos efectivos se piden a Platform, no se deducen del estado de la suscripción en el cliente**. POS puede cachear la respuesta para operar offline, con la política de §4.

### 3.6 Licenciamiento técnico

- `GET /api/v1/installations/{id}/licenses`, `POST /api/v1/installations/{id}/licenses`.
- Emisión comercial controlada: `POST /api/v1/control/installations/{id}/licenses` (sesión staff, capability `InstallationsManage`). El cuerpo sólo lleva `product_id`; plan, features y vigencia se resuelven con `entitlements` antes de firmar y no se aceptan del cliente HTTP.
- Contrato `LicenseGrant` v1: `version`, `grant_id`, `installation_id`, `organization_id`, `plan`, `features`, `issued_at`, `valid_until`, serializado en JSON canónico y firmado con **Ed25519**.
- La clave privada nunca sale del backend. El verificador de POS sólo necesita la clave pública.
- La emisión productiva está **deshabilitada** (`ALLOW_DEV_ISSUANCE=false` y `HIQNET_PLATFORM_ALLOW_LICENSE_ISSUANCE` desactivada por defecto). Las rutas DEV no son un mecanismo comercial.

## 4. Operación offline

POS conserva la autoridad sobre sus datos locales (SQLite) y debe seguir vendiendo sin red.

Requisitos que Platform impone, no implementa:

1. **Licencia cacheada con `valid_until`.** POS debe seguir operando dentro de la ventana firmada aunque Platform no responda.
2. **Ventana de gracia explícita.** Definir en POS cuánto tiempo tolera operar con una licencia vencida antes de degradar. Platform no impone esa política.
3. **Renovación con reintentos idempotentes.** POS debe reintentar sin duplicar suscripciones ni grants.
4. **Resolución de conflictos.** Si Platform suspende una suscripción mientras POS está offline, la suspensión debe aplicarse al reconectar, sin borrar datos locales.
5. **Nunca borrar datos locales por un fallo de licencia.** El bloqueo es de funcionalidad, no de datos.

## 5. Contratos pendientes antes de un POS productivo

| Contrato | Motivo | Bloqueante para |
|---|---|---|
| Custodia y rotación de la clave Ed25519 | Hoy la clave viene de configuración; la emisión comercial exige firmante y habilitación explícita de pruebas | Emisión productiva de licencias |
| Elegibilidad de emisión | **Resuelta**: suscripción activa, producto habilitado, organización activa, instalación perteneciente y capability del actor se verifican antes de firmar | — |
| Auditoría de emisión | **Resuelta**: `license.issued` y `license.issuance_denied` en `staff_audit_event`, transaccional con el registro del grant | — |
| Duplicados y reintentos | **Resuelto**: unicidad (instalación, suscripción); la repetición **recupera el grant original** (200, `recovered=true`) sin re-firmar ni duplicar, auditada como `license.recovered`; emisiones concurrentes convergen al mismo documento; reemisión sólo tras renovación (suscripción nueva) | — |
| Política de renovación offline | Ventana de gracia, reintentos y conflicto | Operación offline en campo |
| Revocación de licencia | POS debe detectar una licencia revocada al reconectar | Soporte y abuso |
| Contrato de errores de integración | Códigos estables para POS | UX de POS |

## 6. Riesgos pendientes que POS hereda

1. **MFA/step-up ausente.** Una cuenta staff comprometida sigue siendo el mayor riesgo de Platform.
2. **Advisories de dependencias.** `libsqlite3-sys` y `rsa` se cierran con `sqlx 0.9`; POS usará SQLite, así que **debe resolverse en POS**, no diferirse.
3. **Sin retención de auditoría.** El volumen crecerá con cada instalación activa.
4. **Sin detección fiable de clones.** POS no debe asumir que Platform puede identificar un dispositivo duplicado.
5. **Rate limiters por instancia.** Afecta a enrolamientos masivos.
6. **Product Registry sin UI de alta.** No afecta a POS, pero limita la operación administrativa.

## 7. Pruebas que POS debe ejecutar contra Platform

Antes de un piloto comercial:

1. Enrolamiento completo con un dispositivo real y proof Ed25519.
2. Código expirado, inválido, reutilizado y organización ajena.
3. Doble aprobación y reintentos concurrentes.
4. Suspensión de suscripción → POS pierde derechos al reconectar, sin perder datos.
5. Reactivación → POS recupera derechos.
6. Expiración lógica con `status` persistido `active`.
7. Renovación idempotente ante reintentos y cortes de red.
8. Verificación de licencia con la clave pública, incluyendo firma alterada y `valid_until` vencido.
9. Operación offline durante toda la ventana de gracia y reconexión posterior.
10. Rehost autorizado y rechazo de rehost sin doble autoridad.
11. Aislamiento: un dispositivo de una organización no accede a datos de otra.

## 8. Prioridades de la siguiente etapa (POS)

Platform queda cerrada. El trabajo se concentra en:

- Arquitectura Rust + React + Tauri 2.
- Persistencia SQLite y migraciones locales.
- Ventas, inventario, compras, gastos, merma, devoluciones.
- Cajas y arqueos.
- Respaldos y recuperación local.
- Operación offline y sincronización diferida (frente separado).
- Compatibilidad Windows, Linux y Android.
- Integración segura con Platform: identidad local, enrolamiento, entitlements y licencia.
- Licenciamiento productivo con custodia de clave.
- Piloto comercial.

La sincronización cloud **no** es un requisito oculto de esta transición: es un frente independiente que se planifica cuando POS tenga operación local estable.

## 9. Lo que no se debe hacer

- No reabrir decisiones de arquitectura ya ratificadas por preferencia estética.
- No añadir funcionalidad general a Platform "ya que estamos".
- No introducir dependencia runtime de Platform para que POS pueda vender.
- No usar endpoints DEV como sustituto de un contrato productivo.
- No declarar POS listo por el hecho de que Platform esté en Beta.
