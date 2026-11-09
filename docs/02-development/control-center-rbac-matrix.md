# Control Center — Matriz de roles y blast radius

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Relacionado:** [ADR-0006](../01-architecture/adr-0006-staff-rbac.md), [Threat model](../01-architecture/control-center-threat-model.md)

Esta matriz define, por rol, qué puede **leer**, qué puede **modificar** y qué **nunca** puede hacer. La autoridad final es el backend; la UI sólo refleja permisos.

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
- `security_auditor` puede mutar configuración de seguridad y triage, no datos de negocio.
- 4-eyes se difiere a C6.

## 5. Propuesta de enmienda Fase 1 (Beta)

Esta sección complementa la matriz anterior y no cambia los checks de roles. Una vez ratificada, prevalecerá sobre redacciones previas que situaran triage mutable o 4-eyes en C6. Queda pendiente de revisión.

- **Capabilities comerciales Beta:** no se crean nuevas. Planes usan `products.read`/`products.manage`; suscripciones, entitlements y OrganizationProduct usan `organizations.read`/`organizations.manage`.
- **Consecuencia:** `operator`, `support` y `read_only` pueden leer planes/suscripciones/entitlements; sólo `super_admin` y `platform_admin` pueden crear/editar planes y transicionar suscripciones.
- **`security.manage`:** reservada; C6 es read-only y no la consume. No concede `installations.rehost` ni acceso adicional a los read models C6.
- **`security_auditor`:** conserva `security.read`, `security.manage` y `audit.read`; en C6 sólo usa lectura. No tiene `installations.rehost`.
- **`security.read`/`audit.read`:** concedidas a todos los roles según la matriz ratificada. Riesgo de confidencialidad interna aceptado para Beta controlada, condicionado a metadata saneada y superficie staff. Ver [Seguridad Beta](platform-beta-security.md).
- **4-eyes:** diferido a una fase futura con mutación crítica; C6 no lo requiere.

En caso de conflicto con la redacción histórica, prevalecerá esta enmienda una vez ratificada; PR #22 (C6) alinea la redacción anterior.
