# ADR-0006 — RBAC interno de personal HiQNet

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Fase:** C0 — Architecture Foundation
**Relacionado:** [ADR-0005](adr-0005-staff-identity.md), [ADR-0007](adr-0007-product-project-model.md)

## 1. Contexto

El RBAC comercial (`owner/admin/member`) pertenece a organizaciones cliente y **no** aplica al personal interno. El Control Center necesita un RBAC propio con mínimo privilegio y capacidad de evolucionar hacia permisos granulares (capabilities).

## 2. Decisión propuesta

### 2.1 Roles iniciales

| Rol                | Intención                                                                                         |
| ------------------ | ------------------------------------------------------------------------------------------------- |
| `super_admin`      | Administración de la plataforma y del propio staff. Sujeto a auditoría y step-up; **sin bypass**. |
| `platform_admin`   | Operación de productos, organizaciones, cuentas, instalaciones y despliegues. No gestiona staff.  |
| `operator`         | Operación diaria: instalaciones y despliegues. Lectura amplia; mutaciones acotadas.               |
| `support`          | Atención al cliente: lectura de organizaciones/cuentas/instalaciones y acciones no destructivas.  |
| `security_auditor` | Lectura global + gestión futura de eventos de seguridad. No modifica datos de negocio.                   |
| `read_only`        | Lectura global sin mutaciones.                                                                    |

### 2.2 Capabilities (nombres propuestos)

```
products.read         products.manage
organizations.read    organizations.manage
accounts.read         accounts.manage
installations.read    installations.manage    installations.rehost
subscriptions.read    subscriptions.manage
deployments.read      deployments.manage
security.read         security.manage
audit.read
staff.manage
```

- Los roles son **presets** de capabilities. En C1 se persiste el rol; el modelo debe permitir añadir capabilities explícitas y overrides en fases posteriores sin migración destructiva.
- Los **proyectos del registro** (`Project`) se gestionan con `products.read`/`products.manage`; no se introducen capabilities `projects.*` en C2. Si una fase posterior necesita delegar proyectos de forma independiente, se añadirá mediante cambio explícito de este ADR y de la matriz.
- **`installations.rehost`** (C5) separa el reemplazo de identidad criptográfica de la operación administrativa (`installations.manage`). Sólo `super_admin` y `platform_admin` lo reciben; `operator` puede suspender/reactivar pero no reautorizar una identidad de dispositivo.
- **`security.manage`** queda **reservada** para una futura superficie mutable de seguridad. C6 inicial es read-only: no expone ninguna operación que la consuma, no concede acceso adicional a los read models de C6, no concede `installations.rehost` y no implica incident management ni configuración de seguridad. Su uso se difiere hasta que exista un dominio mutable real que lo justifique.

### 2.3 Reglas

- Deny by default: una capability no concedida se rechaza con `403`.
- Un rol no concede nunca `staff.manage` salvo `super_admin`.
- Las acciones destructivas exigen confirmación explícita (typed confirmation) y, en el futuro, step-up.
- `super_admin` no evita autenticación, auditoría ni controles de sesión.
- El frontend puede ocultar acciones, pero la autoridad final es el backend.

## 3. Matriz propuesta (sujeta a aprobación)

Ver [matriz de roles](../02-development/control-center-rbac-matrix.md) para la tabla detallada `qué puede leer / qué puede modificar / qué nunca puede hacer`.

Resumen de capabilities:

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

## 4. Consecuencias

- Middleware/extractor staff que resuelve `staff_account` + `capabilities` en cada request.
- `403` neutral para capability insuficiente; `404` neutral para recurso inexistente/fuera de alcance.
- Cada mutación registra `staff_audit_event` con actor, acción, recurso, resultado y organización/producto cuando aplique.
- La autorización no se cachea de forma que sobreviva a una revocación.

## 5. Decisiones ratificadas

- **Roles finales:** `super_admin`, `platform_admin`, `operator`, `support`, `security_auditor`, `read_only`.
- **Matriz capability × rol:** la de la sección 3.
- **`security_auditor`:** conserva `security.read`, `security.manage` y `audit.read`. En C6 inicial usa sólo `audit.read` y `security.read`; `security.manage` no tiene superficie HTTP C6. No modifica datos de negocio y **no** tiene `installations.rehost`.
- **4-eyes:** se difiere a la futura fase que introduzca una mutación administrativa o de seguridad suficientemente crítica. C6 inicial es read-only y no requiere doble aprobación; en C1/C2 basta confirmación explícita y auditoría. Esta frase sustituye la decisión previa que situaba 4-eyes en C6.

## 6. Riesgos

- Roles demasiado amplios aumentan el blast radius de una cuenta comprometida.
- Un RBAC basado sólo en rol no cubre excepciones; se documenta la evolución a capabilities explícitas.
- Sin MFA en C1, el robo de sesión staff es el riesgo principal; se mitiga con sesión corta y revocación inmediata.

## 7. Propuesta de enmienda Fase 1 (Beta)

Esta sección complementa §5 y, una vez ratificada, prevalecerá sobre cualquier redacción anterior que situara 4-eyes en C6 o que implicara una superficie mutable de seguridad en C6. Queda pendiente de revisión.

- **`security.manage`:** reservada para una futura superficie mutable de seguridad. C6 inicial es read-only y no la consume; no concede acceso adicional a los read models de C6 ni concede `installations.rehost`, y no implica incident management ni configuración de seguridad.
- **`security_auditor`:** conserva `security.read`, `security.manage` y `audit.read`; en C6 sólo usa las dos de lectura. No tiene `installations.rehost`.
- **4-eyes:** se difiere a la futura fase que introduzca una mutación administrativa o de seguridad suficientemente crítica. C6 es read-only y no requiere doble aprobación.
- **Dominio comercial Beta:** no se crean capabilities nuevas. Planes usan `products.read`/`products.manage`; suscripciones y entitlements usan `subscriptions.read`/`subscriptions.manage`; OrganizationProduct usa `organizations.manage`. Ver [ADR-0011](adr-0011-commercial-domain.md).
- **Lectura de auditoría/seguridad:** `security.read` y `audit.read` se mantienen para todos los roles según la matriz ratificada. Es un riesgo de confidencialidad interna aceptado para la Beta controlada, condicionado a metadata saneada y superficie staff. Ver [Seguridad Beta](../02-development/platform-beta-security.md).

En caso de conflicto con la redacción histórica de §5, prevalecerá esta enmienda una vez ratificada; PR #22 (C6) alinea la redacción anterior.
