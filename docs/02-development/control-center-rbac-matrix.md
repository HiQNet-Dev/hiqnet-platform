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
| `security_auditor` | Todo, incluida auditoría y eventos | Nada en C6: `security.manage` reservada para una futura superficie mutable; C6 es read-only | Mutar datos de negocio (organizaciones, cuentas, instalaciones) |
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
- **`security.manage` (reservada):** existe en la matriz, pero C6 inicial es read-only y no expone ninguna operación que la consuma. No concede acceso adicional a los read models de C6, no concede `installations.rehost` y no implica incident management ni configuración de seguridad. Su uso se difiere hasta que exista un dominio mutable real.
- **Read models C6:** Audit usa `audit.read`; Security (Platform events y Staff authentication) usa `security.read`. `support` y `read_only` conservan ambas lecturas según la matriz ratificada; es una decisión consciente con blast radius documentado.

## 4. Decisiones ratificadas

- Roles definitivos: los de la sección 1.
- `support` no tiene mutaciones en C1.
- `security_auditor` conserva `security.read`, `security.manage` y `audit.read`; en C6 inicial sólo usa las dos de lectura. No tiene `installations.rehost`.
- 4-eyes se difiere a la futura fase que introduzca una mutación administrativa o de seguridad suficientemente crítica. C6 inicial es read-only y no requiere doble aprobación.
