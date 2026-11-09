# Control Center — Matriz de roles y blast radius

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Relacionado:** [ADR-0006](../01-architecture/adr-0006-staff-rbac.md), [Threat model](../01-architecture/control-center-threat-model.md)

Esta matriz define, por rol, qué puede **leer**, qué puede **modificar** y qué **nunca** puede hacer. La autoridad final es el backend; la UI sólo refleja permisos.

## 0. Enmienda Fase 1 (Beta)

Esta sección complementa §1–§4 y no cambia los checks de roles. Queda pendiente de revisión y no altera las decisiones C6 de `security.manage` ni 4-eyes, que se alinean con PR #22.

- **Capabilities comerciales Beta:** no se crean nuevas. Planes usan `products.read`/`products.manage`; suscripciones y entitlements usan `subscriptions.read`/`subscriptions.manage`; OrganizationProduct usa `organizations.manage`.
- **Consecuencia:** `operator`, `support`, `security_auditor` y `read_only` pueden leer planes/suscripciones/entitlements; sólo `super_admin` y `platform_admin` tienen `products.manage` y `subscriptions.manage`, y pueden crear/editar planes y transicionar suscripciones.
- **`security.read`/`audit.read`:** concedidas a todos los roles según la matriz ratificada. Riesgo de confidencialidad interna aceptado para Beta con staff interno y metadata saneada; no para contratistas o audiencia amplia. Ver [Seguridad Beta](platform-beta-security.md).
- **Customer Portal:** no accede a `/control`, auditoría/seguridad ni a datos de otras organizaciones; sólo lectura comercial de su propia organización.

## 1. Matriz operativa

| Rol                | Puede leer                         | Puede modificar                                                                                 | Nunca puede                                                                                     |
| ------------------ | ---------------------------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `super_admin`      | Todo                               | Productos, organizaciones, cuentas, instalaciones, suscripciones, despliegues, seguridad, staff | Saltarse autenticación, auditoría o controles de sesión; acciones destructivas sin confirmación |
| `platform_admin`   | Todo                               | Productos, organizaciones, cuentas, instalaciones (incluido rehost), suscripciones, despliegues | Gestionar staff; mutar configuración de seguridad                                               |
| `operator`         | Todo                               | Instalaciones (suspender/reactivar), despliegues                                                | Reautorizar identidad de dispositivo (`installations.rehost`); gestionar productos, organizaciones, cuentas, staff o seguridad |
| `support`          | Todo (sin secretos)                | Nada destructivo (acciones de soporte no destructivas, si se aprueban)                          | Mutar cuentas/organizaciones/productos, gestionar staff o seguridad                             |
| `security_auditor` | Todo, incluida auditoría y eventos | Configuración de seguridad y triage de eventos                                                  | Mutar datos de negocio (organizaciones, cuentas, instalaciones)                                 |
| `read_only`        | Todo (sin secretos)                | Nada                                                                                            | Cualquier mutación                                                                              |

## 2. Detalle por capability

| Capability           | super_admin | platform_admin | operator | support | security_auditor | read_only |
| -------------------- | ----------- | -------------- | -------- | ------- | ---------------- | --------- |
| products.read        | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| products.manage      | ✓           | ✓              | —        | —       | —                | —         |
| organizations.read   | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| organizations.manage | ✓           | ✓              | —        | —       | —                | —         |
| accounts.read        | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| accounts.manage      | ✓           | ✓              | —        | —       | —                | —         |
| installations.read   | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| installations.manage | ✓           | ✓              | ✓        | —       | —                | —         |
| installations.rehost | ✓           | ✓              | —        | —       | —                | —         |
| subscriptions.read   | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| subscriptions.manage | ✓           | ✓              | —        | —       | —                | —         |
| deployments.read     | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| deployments.manage   | ✓           | ✓              | ✓        | —       | —                | —         |
| security.read        | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| security.manage      | ✓           | —              | —        | —       | ✓                | —         |
| audit.read           | ✓           | ✓              | ✓        | ✓       | ✓                | ✓         |
| staff.manage         | ✓           | —              | —        | —       | —                | —         |

Leyenda: `✓` permitido · `—` denegado.

## 3. Reglas transversales

- **Deny by default.** Una capability no concedida se rechaza con `403`.
- **404 neutral** para recursos inexistentes o fuera de alcance (no distinguir cross-tenant).
- **Auditoría obligatoria** en toda mutación: actor, acción, recurso, resultado, `organization_id`/`product_id` cuando aplique.
- **Step-up** (futuro) para acciones críticas; en C1, confirmación tipada.
- **Sin bypasses:** `super_admin` se autentica, se audita y respeta sesión/Origin.
- **Least privilege:** ante duda, denegar y revisar.
- **`installations.rehost` (C5):** capability independiente de `installations.manage`; reemplaza la identidad criptográfica de una instalación. Exige además un `enrollment_request_id` aprobado por el owner comercial; el staff no sustituye esa autoridad.

## 4. Decisiones ratificadas

- Roles definitivos: los de la sección 1.
- `support` no tiene mutaciones en C1.
- `security_auditor` conserva `security.read`, `security.manage` y `audit.read`; en C6 inicial sólo usa las dos de lectura. No tiene `installations.rehost`.
- 4-eyes se difiere a la futura fase que introduzca una mutación administrativa o de seguridad suficientemente crítica. C6 inicial es read-only y no requiere doble aprobación.
