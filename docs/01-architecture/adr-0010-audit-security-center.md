# ADR-0010 — Audit / Security Center (C6)

**Estado:** ACCEPTED — contrato ratificado. C6 **NO implementado**; C6.1 no iniciado.
**Fecha:** 2026-11-09
**Fase:** C6 — Audit / Security Center (sólo contrato)
**Relacionado:** [ADR-0004](adr-0004-control-center-boundaries.md), [ADR-0005](adr-0005-staff-identity.md), [ADR-0006](adr-0006-staff-rbac.md), [ADR-0009](adr-0009-pos-installation-control.md), [Matriz RBAC](../02-development/control-center-rbac-matrix.md), [Roadmap](control-center-roadmap.md)

> Este documento fija el contrato de C6 antes de implementar. No crea endpoints, OpenAPI, tablas ni migration `0010`. Las decisiones `D-C6-01..D-C6-12` quedan **APROBADAS** a nivel documental; C6 sigue **NO IMPLEMENTADO** hasta que se ejecuten C6.1–C6.4.

## 1. Contexto

C0–C5 están cerrados e integrados en `main` (`14a10f0`). El Control Center ya expone Products, Organizations, Accounts e Installations con `staffSession`, RBAC por capability y auditoría append-only.

C6 añade observabilidad administrativa de lo que ya ocurre en la plataforma:

- quién hizo qué sobre qué recurso (`staff_audit_event`);
- qué eventos de seguridad de identidad/instalación se produjeron (`commercial_security_event`);
- qué eventos de autenticación staff se produjeron (`staff_auth_event`).

El inventario C6.0A verificó que **no existe aún ningún read model ni endpoint** para estas tablas: sólo writers y lecturas de test. C6 será la primera superficie de lectura.

## 2. Alcance

C6 inicial es **READ ONLY**:

- `GET` de listado y detalle para tres datasets;
- sin `POST`, `PATCH` ni `DELETE` de eventos;
- sin acknowledge, resolve, dismiss, ban, block, revoke-all, force logout, incidentes, casos, alertas ni configuración de seguridad;
- sin mutaciones que justifiquen 4-eyes.

C6 no implementa retención, particionado, archivo ni purga.

## 3. Hechos actuales (verificados)

### 3.1 `staff_audit_event`

- Append-only real: trigger `staff_audit_event_append_only` sobre `UPDATE` y `DELETE`.
- Columnas: `id`, `staff_account_id?`, `action`, `resource_type`, `resource_id?`, `organization_id?`, `product_id?`, `result`, `occurred_at`, `metadata`.
- `action` es **texto abierto** (no enum). `resource_id` es polimórfico sin FK.
- `result` admite `success|denied|failure`, pero los writers actuales sólo producen `success`.
- 10 acciones reales: `product_created`, `product_updated`, `project_created`, `project_updated`, `installation_suspended`, `installation_reactivated`, `installation_rehost_authorized`, `installation_identity_retired`, `installation_binding_revoked`, `installation_rehost_completed`.
- `organization_id` es NULL en eventos de producto/proyecto; sólo las instalaciones lo pueblan.
- Índices existentes: `(staff_account_id, occurred_at DESC)` y `(resource_type, resource_id)`.

### 3.2 `commercial_security_event`

- Append-only real: trigger `commercial_security_event_append_only`.
- Columnas: `id`, `kind`, `organization_id?`, `account_id?` (cuenta **comercial**), `installation_id?`, `metadata`, `created_at`.
- CHECK cerrado de 9 `kind`: `enrollment_conflict`, `duplicate_key`, `tenant_change_attempt`, `replay_rejected`, `rehost_performed`, `identity_revoked`, `authorization_revoked_during_proof`, `idempotency_conflict`, `clone_suspected`.
- Usados hoy: `duplicate_key`, `tenant_change_attempt`, `replay_rejected`, `authorization_revoked_during_proof`, `rehost_performed`, `identity_revoked`.
- Definidos sin writer: `enrollment_conflict`, `idempotency_conflict`. Conceptual sin implementar: `clone_suspected`.
- Índices existentes: sólo `(organization_id, created_at DESC)`.

### 3.3 `staff_auth_event`

- Append-only real: trigger `staff_auth_event_append_only`.
- Columnas: `id`, `account_id?`, `action`, `occurred_at`, `ip_hash?`, `metadata`.
- CHECK de 6 acciones: `login_success`, `login_failure`, `logout`, `session_revoked`, `account_locked`, `password_changed`.
- Writers reales: `login_success`, `login_failure`, `logout`.
- Sin writers productivos: `session_revoked`, `account_locked`, `password_changed`.
- `ip_hash` existe pero **no se puebla**; `staff_session.ip_hash` y `user_agent_hash` tampoco.
- Índices existentes: `(account_id, occurred_at DESC)` y `(action, occurred_at DESC)`.

### 3.4 `commercial_audit_event`

- Existe y es append-only, pero representa auditoría del **dominio comercial** (actor = cuenta comercial), no mutaciones administrativas staff. **Fuera de C6** (§21).

### 3.5 Límites conocidos

- `TRUNCATE` no está protegido por los triggers row-level (deuda de hardening DB/privilegios).
- No hay política de retención, archivado ni particionado.
- `staff_auth_event` no dispone de IP/UA fiables.
- `installation_identity_retired` guarda `{"generation":"active"}` (placeholder), no una generación real.

## 4. Dominios

### 4.1 Audit

- Fuente única: `staff_audit_event`.
- Responde: ¿quién (staff) hizo qué, cuándo, sobre qué recurso, en qué organización/producto, con qué resultado?
- Read model, endpoint, filtros y metadata propios.

### 4.2 Security — Platform / comercial

- Fuente única: `commercial_security_event`.
- Responde: ¿qué evento de seguridad de identidad/instalación/enrolamiento/rehost ocurrió, sobre qué organización/instalación/cuenta comercial?
- Read model, endpoint, filtros y metadata propios.
- `account` es siempre **cuenta comercial**, nunca actor staff.

### 4.3 Security — Staff authentication

- Fuente única: `staff_auth_event`.
- Responde: ¿qué evento de autenticación staff ocurrió y sobre qué cuenta staff?
- Read model, endpoint, filtros y metadata propios.
- Es parte de Security en C6, pero conserva su tabla, DTO, endpoint y semántica (ADR-0005: `staff_auth_event` ≠ `staff_audit_event`).

### 4.4 Regla de no-unión

**No se hace `UNION` físico ni conceptual.** No se crea un pseudo-evento genérico. Cada fuente conserva su tabla, DTO, endpoint, filtros, metadata y semántica. La agrupación es sólo de información architecture (Security contiene dos sub-superficies), no de contrato de datos.

`commercial_audit_event` queda fuera de C6 inicial.

## 5. RBAC

- Audit: `audit.read`.
- Commercial/Platform Security: `security.read`.
- Staff Authentication: `security.read`.
- `security.manage` **no se consume** en C6.

Reglas:

- Deny by default, `403` neutral.
- La matriz runtime actual no se modifica.
- `audit.read` y `security.read` están concedidas a los seis roles (incluidos `support` y `read_only`). C6 **respeta conscientemente** la matriz ratificada; una reducción futura requiere ADR explícito.
- `security_auditor` conserva `security.read`, `security.manage` y `audit.read`; en C6 usa sólo las dos de lectura. **No** tiene `installations.rehost`.
- No se crean `audit.manage`, `staff_auth.read` ni `security_event.read`.

### 5.1 `security.manage`

Se mantiene la capability, el enum Rust y la matriz. Queda documentada como **reservada**:

- C6 no expone ninguna operación que la consuma;
- no concede acceso adicional a los read models de C6;
- no concede `installations.rehost`;
- no implica incident management ni configuración de seguridad;
- su uso queda diferido hasta que exista un dominio mutable real.

### 5.2 4-eyes

C6 inicial es read-only y **no** introduce operaciones que requieran doble aprobación. 4-eyes se difiere a la futura fase que introduzca una mutación administrativa o de seguridad suficientemente crítica. Esta ADR enmienda la frase previa de ADR-0006 que lo situaba en C6.

## 6. API

```
GET /api/v1/control/audit
GET /api/v1/control/audit/{event_id}
GET /api/v1/control/security-events
GET /api/v1/control/security-events/{event_id}
GET /api/v1/control/staff-auth-events
GET /api/v1/control/staff-auth-events/{event_id}
```

- `operation_id`: `listControlAudit`, `getControlAudit`, `listControlSecurityEvents`, `getControlSecurityEvent`, `listControlStaffAuthEvents`, `getControlStaffAuthEvent`.
- Sólo `staffSession`; namespace `/api/v1/control/*`.
- Sin endpoints comerciales, sin `devAdminToken`, sin legacy admin.
- No se usa `/auth/events` para evitar confusión con autenticación.

### 6.1 Errores

```
400 validation_error
401 unauthorized
403 forbidden
404 not_found
503 storage_error
```

- `404` neutral para recurso inexistente.
- No filtrar existencia fuera de scope, detalles internos de BD ni secretos.
- `400` para cursor inválido/reutilizado, filtros inválidos, UUID inválido, límite fuera de rango o fechas inválidas.

## 7. Read models

### 7.1 Audit

`ControlAuditEventSummaryResponse`:

```
id
occurred_at
actor { account_id, display_name } | null
action
resource_type
resource_id | null
resource_label | null
organization { id, display_name } | null
product { id, key, name } | null
result (success|denied|failure)
```

`ControlAuditEventDetailResponse` = summary +:

```
actor { account_id, display_name, email, status } | null
metadata (sanitizada, tipada por action)
```

`ControlAuditEventListResponse` = `{ items, next_cursor }`.

### 7.2 Commercial / Platform Security

`ControlSecurityEventSummaryResponse`:

```
id
occurred_at            (commercial_security_event.created_at)
kind                   (enum cerrado)
organization { id, display_name } | null
installation { id, device_label } | null
account { id, display_name } | null      (cuenta comercial)
```

`ControlSecurityEventDetailResponse` = summary +:

```
account { id, display_name, email } | null
metadata (sanitizada por kind)
```

`ControlSecurityEventListResponse` = `{ items, next_cursor }`.

### 7.3 Staff Authentication

`ControlStaffAuthEventSummaryResponse`:

```
id
occurred_at
action                 (enum cerrado)
account { id, display_name } | null
```

`ControlStaffAuthEventDetailResponse` = summary +:

```
account { id, display_name, email, status } | null
metadata (sanitizada por action)
```

`ControlStaffAuthEventListResponse` = `{ items, next_cursor }`.

### 7.4 Resource labels (Audit)

- Sólo resolución whitelisted y explícita:
  - `resource_type = "product"` → `product.name`
  - `resource_type = "project"` → `project.name`
  - `resource_type = "installation"` → `installation.device_label`
- Sin SQL dinámico a partir de `resource_type`.
- Cualquier otro tipo o recurso no encontrado → `resource_label = null`.
- No N+1: resolución en la misma query del listado/detalle.

## 8. Sanitización de metadata

**Nunca** se envía JSONB arbitrario al browser. Whitelist por dataset y clave; fallback `{}`.

### 8.1 Audit (por action)

| action | metadata expuesta |
|---|---|
| `installation_suspended` | `before`, `after` |
| `installation_reactivated` | `before`, `after` |
| `installation_rehost_authorized` | `enrollment_request_id`, `reason`, `expires_at` |
| `installation_binding_revoked` | `reason` |
| `installation_rehost_completed` | `enrollment_request_id`, `identity_id` |
| `installation_identity_retired` | `{}` (no exponer el placeholder `generation:"active"` como generación real) |
| `product_created` / `product_updated` / `project_created` / `project_updated` | `{}` |
| desconocida | `{}` |

### 8.2 Commercial Security (por kind)

| kind | metadata expuesta |
|---|---|
| `rehost_performed` | `enrollment_request_id` |
| `identity_revoked` | `reason` |
| resto | `{}` |

### 8.3 Staff Authentication (por action)

| action | metadata expuesta |
|---|---|
| `login_failure` | `reason` (whitelist: `unknown`, `no_credential`, `locked`, `bad_password`, `credential_changed`, `account_unavailable`, `unexpected`) |
| `login_success` | `{}` |
| `logout` | `{}` |
| `session_revoked` / `account_locked` / `password_changed` | `{}` hasta que exista writer y contrato de metadata |
| desconocida | `{}` |

No se exponen `ip_hash`, `user_agent_hash`, `token_hash`, `session_id`, `password_hash`, nonces, device codes ni user codes.

## 9. Clasificación de datos (contractual)

| Dato | Clasificación | Política UI |
|---|---|---|
| `display_name` (staff/comercial/instalación/producto/organización) | SAFE TO DISPLAY | list + detail |
| `email` staff | DETAIL ONLY | detail |
| `email` comercial | DETAIL ONLY | detail |
| UUIDs (`id`, `account_id`, `resource_id`, `organization_id`, `product_id`, `installation_id`, `enrollment_request_id`, `identity_id`) | SAFE TO DISPLAY | list/detail |
| `organization`, `product`, `installation` (refs) | SAFE TO DISPLAY | list + detail |
| `commercial account` (ref; `account`) | SAFE TO DISPLAY | list + detail |
| `reason` (texto libre) | DETAIL ONLY | detail |
| `metadata` JSONB | SANITIZE — whitelist | detail únicamente |
| `fingerprint` | SAFE TO DISPLAY (hash de clave pública) | sólo si se resuelve instalación; no está en metadata C6 |
| `public_key` | INTERNAL ONLY | no exponer |
| `ip_hash` / `user_agent_hash` | INTERNAL ONLY (además no poblados) | no exponer |
| `token_hash` | SECRET / NEVER EXPOSE | no exponer |
| `session id` | SECRET / NEVER EXPOSE | no exponer |
| `password_hash` | SECRET / NEVER EXPOSE | no exponer |
| `device_code` / `user_code` | SECRET / NEVER EXPOSE | no exponer |
| `nonce` | SECRET / NEVER EXPOSE | no exponer |

## 10. Paginación

- Keyset, descendente, `limit` 1..100 (default 50), `next_cursor` opaco.
- Audit: `ORDER BY occurred_at DESC, id DESC`.
- Commercial Security: `ORDER BY created_at DESC, id DESC`.
- Staff Auth: `ORDER BY occurred_at DESC, id DESC`.
- Sin `OFFSET`, sin listados sin límite.

### 10.1 Cursor versionado

JSON base64url:

```
{ "v": 1, "k": "audit" | "security" | "staff_auth", "t": <nanos>, "i": <uuid>, "f": <hash de filtros normalizados> }
```

El cursor es inválido (`400 validation_error`) si:

- se reutiliza en otro endpoint (`k` distinto);
- se usa con otros filtros (`f` distinto);
- el formato es corrupto o supera la longitud máxima;
- la versión es desconocida.

## 11. Filtros

### 11.1 Audit (contrato API)

`staff_account_id`, `action` (exacto), `resource_type`, `resource_id`, `organization_id`, `product_id`, `result`, `from`, `to`.

No búsqueda libre sobre JSONB. Sin trigram inicialmente.

### 11.2 Commercial Security (contrato API)

`kind`, `organization_id`, `installation_id`, `account_id`, `from`, `to`.

No `severity`, `risk`, `status`, `resolved`, `acknowledged` (no existen).

### 11.3 Staff Authentication (contrato API)

`account_id`, `action`, `from`, `to`.

`action` admite el enum completo (`login_success|login_failure|logout|session_revoked|account_locked|password_changed`) aunque tres valores no tengan writers todavía. La UI no debe sugerir que existen eventos históricos que no existen.

## 12. Matriz query → index

Se optimizan las queries dominantes; no se crea un índice por cada parámetro. No se eliminan índices existentes en C6.

**Regla contractual de ejecución:** toda ordenación, keyset y `LIMIT` de C6 se ejecutan en **PostgreSQL**. Los índices existentes que no cubran completamente el `ORDER BY` pueden requerir un sort del planner, pero el backend **nunca** carga un conjunto ilimitado para ordenarlo o paginarlo en la aplicación. Todas las queries de listado (Audit, Commercial Security y Staff Auth) usan keyset + `LIMIT` acotado en la base de datos.

| # | Query | Ordering | Filtros | Índice existente | ¿Nuevo índice? | Justificación |
|---|---|---|---|---|---|---|
| A1 | Audit global | `occurred_at DESC, id DESC` | — | — | `(occurred_at DESC, id DESC)` | listado principal |
| A2 | Audit por actor | `occurred_at DESC, id DESC` | `staff_account_id` | `(staff_account_id, occurred_at DESC)` | No | reutilizar; el desempate por `id` sólo ordena empates |
| A3 | Audit por action | `occurred_at DESC, id DESC` | `action` | — | `(action, occurred_at DESC, id DESC)` | filtro dominante |
| A4 | Audit por organization | `occurred_at DESC, id DESC` | `organization_id` | — | `(organization_id, occurred_at DESC, id DESC)` | filtro dominante |
| A5 | Audit por product | `occurred_at DESC, id DESC` | `product_id` | — | `(product_id, occurred_at DESC, id DESC)` | filtro dominante |
| A6 | Audit por resource | `occurred_at DESC, id DESC` | `resource_type, resource_id` | `(resource_type, resource_id)` | No | El índice existente acota el conjunto por recurso. PostgreSQL mantiene `ORDER BY occurred_at DESC, id DESC` y aplica keyset + `LIMIT`; C6 no ordena ni pagina en memoria. |
| A7 | Audit por result/rango | `occurred_at DESC, id DESC` | `result`, `from/to` | A1 | No | `result` es baja cardinalidad; A1 cubre el rango |
| S1 | Security global | `created_at DESC, id DESC` | — | — | `(created_at DESC, id DESC)` | listado principal |
| S2 | Security por organization | `created_at DESC, id DESC` | `organization_id` | `(organization_id, created_at DESC)` | No | reutilizar |
| S3 | Security por kind | `created_at DESC, id DESC` | `kind` | — | `(kind, created_at DESC, id DESC)` | filtro dominante |
| S4 | Security por installation | `created_at DESC, id DESC` | `installation_id` | — | `(installation_id, created_at DESC, id DESC)` | filtro dominante |
| S5 | Security por account | `created_at DESC, id DESC` | `account_id` | — | `(account_id, created_at DESC, id DESC)` | filtro dominante |
| T1 | Staff auth global | `occurred_at DESC, id DESC` | — | — | `(occurred_at DESC, id DESC)` | listado principal |
| T2 | Staff auth por account | `occurred_at DESC, id DESC` | `account_id` | `(account_id, occurred_at DESC)` | No | reutilizar |
| T3 | Staff auth por action | `occurred_at DESC, id DESC` | `action` | `(action, occurred_at DESC)` | No | reutilizar |

### 12.1 Conjunto mínimo propuesto para `0010`

```
-- Audit
CREATE INDEX staff_audit_event_occurred_idx
    ON staff_audit_event (occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_action_occurred_idx
    ON staff_audit_event (action, occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_org_occurred_idx
    ON staff_audit_event (organization_id, occurred_at DESC, id DESC);
CREATE INDEX staff_audit_event_product_occurred_idx
    ON staff_audit_event (product_id, occurred_at DESC, id DESC);

-- Commercial Security
CREATE INDEX commercial_security_event_created_idx
    ON commercial_security_event (created_at DESC, id DESC);
CREATE INDEX commercial_security_event_kind_created_idx
    ON commercial_security_event (kind, created_at DESC, id DESC);
CREATE INDEX commercial_security_event_installation_created_idx
    ON commercial_security_event (installation_id, created_at DESC, id DESC);
CREATE INDEX commercial_security_event_account_created_idx
    ON commercial_security_event (account_id, created_at DESC, id DESC);

-- Staff Authentication
CREATE INDEX staff_auth_event_occurred_idx
    ON staff_auth_event (occurred_at DESC, id DESC);
```

Nueve índices, ninguno duplicado con los existentes, ninguno `DROP INDEX`. La migración `0010` es append-only (`CREATE INDEX`).

## 13. Migration decision

- **C6 necesita migration `0010`: SÍ.**
- Sólo índices de lectura (§12.1). Sin tablas, columnas, triggers, datos ni retención.
- `0010` **no se crea en C6.0B**; pertenece a C6.1.
- No se crea una migración vacía. No se renombran ni reescriben `0001–0009`.

## 14. Frontend — information architecture

Navegación principal (dos entradas, no tres):

```
Audit
Security
```

```
/control/audit                          list       audit.read
/control/audit/:eventId                 detail     audit.read

/control/security                       → platform (redirect/índice)
/control/security/platform              list       security.read
/control/security/platform/:eventId     detail     security.read
/control/security/staff-auth            list       security.read
/control/security/staff-auth/:eventId   detail     security.read
```

Security usa subnavegación/tabs para distinguir:

```
[Platform events] [Staff authentication]
```

No se crea un tercer módulo top-level `Staff Auth`.

### 14.1 Audit UI

Columnas: `Timestamp · Actor · Action · Resource · Organization · Product · Result`.
Detalle: summary + ids + actor (email/status) + metadata sanitizada.
Filtros UI iniciales: `action`, `result`, `from`, `to`. IDs disponibles en API para deep links/futuro.

### 14.2 Security — Platform UI

Columnas: `Timestamp · Kind · Organization · Installation · Account`.
Sin `severity` y sin colores que la impliquen; badge neutral por `kind`.
Filtros UI iniciales: `kind`, `from`, `to`.

### 14.3 Security — Staff Authentication UI

Columnas: `Timestamp · Staff account · Action · Context`.
`Context` sólo se deriva de metadata sanitizada (`reason`).
No se muestran IP, UA, hashes de sesión ni tokens.
Filtros UI iniciales: `action`, `from`, `to`.

### 14.4 Estados

`loading`, `empty`, `403`, `401` (lifecycle C3.1), `404` de detalle, `storage_error`, cursor inválido (con acción de reset). Reutilizar `AppShell`, `states.tsx`, `.table-wrap`, `.filters`, `.details`, `.badge` y el patrón responsive `data-label` de C5. Sin framework nuevo.

## 15. Testing strategy (a implementar en C6.1–C6.3)

### Backend

- Keyset con empates de timestamp y orden estable; `next_cursor` nulo en la última página.
- Filtros exactos por dataset y combinaciones.
- Cursor inválido: versión, dataset, filtro, base64, longitud, UUID.
- `limit` 0 / >100 → 400.
- 404 neutral para id inexistente.
- RBAC por capability (`audit.read`, `security.read`); ningún endpoint usa `security.manage`.
- Sanitización de metadata: whitelist, claves desconocidas descartadas, sin secretos.
- Inmutabilidad: `UPDATE` y `DELETE` rechazados en las tres tablas.
- Migración `0010`: presencia de los nueve índices.
- HTTP: 200/400/401/403/404/503, RFC3339, JSON de error.
- OpenAPI: generación, snapshot y validador 3.1.

### Frontend

- Unit: formatters de `action`/`kind`, query params, estados.
- E2E con Control API simulada × 1920/1440/1024/768/390:
  - Audit list/detail; Security platform list/detail; Staff auth list/detail;
  - filtros y cursor;
  - RBAC (nav oculta / `AccessDenied` sin capability);
  - 401 → login; 404 → empty; cursor inválido → error;
  - ausencia de secretos visibles;
  - responsive.

### Security

- Test de que ningún DTO expone `token_hash`, `session_id`, `password_hash`, nonces, `ip_hash`, `user_agent_hash` ni JSONB crudo.
- Revisión de la tabla de clasificación §9.

## 16. Security boundaries

- Sólo `staffSession` y `/api/v1/control/*`.
- Sin `commercialSession`, sin `devAdminToken`, sin legacy admin.
- `404` neutral; `403` neutral por capability.
- No se exponen secretos ni hashes de sesión/IP/UA.
- No se permite filtrar por datos inexistentes (severity/risk/status).

## 17. Retención

```
RETENTION POLICY: NO DEFINIDA
```

C6 no implementa purge, particionado, archivado ni job de retención. Riesgo documentado: crecimiento continuo de las tablas; la paginación resuelve coste de consulta, no almacenamiento. Requiere ADR futuro.

## 18. Append-only

`staff_audit_event`, `staff_auth_event` y `commercial_security_event` son append-only para `UPDATE` y `DELETE` mediante triggers actuales. C6 añadirá tests explícitos para esos triggers en C6.1/C6.3.

### 18.1 Gap conocido: `TRUNCATE`

`UPDATE`/`DELETE` están protegidos; `TRUNCATE` **no** lo está por triggers row-level. No se resuelve en C6 ni se crea un trigger artificial. Se registra como deuda de hardening DB/privilegios para una fase posterior.

## 19. Riesgos

- Crecimiento continuo sin retención.
- `audit.read`/`security.read` concedidas a todos los roles (incluidos `support` y `read_only`): blast radius amplio, decisión consciente.
- `TRUNCATE` no bloqueado y ausencia de separación de privilegios documentada.
- `staff_auth_event` incompleto (sin IP/UA, tres acciones sin writers); la UI no debe insinuar datos ausentes.
- `action` de audit es texto abierto: el DTO debe tratarlo como string, no como enum.
- Metadata evoluciona con nuevas acciones/kinds: el mapeo whitelist debe actualizarse en cada writer nuevo.
- Sin tests de inmutabilidad actuales; se añaden en C6.

## 20. Fuera de alcance (D-C6-12)

```
billing, payments, subscriptions, pricing, CFDI
sync control
environments, releases, deployments (C7)
remote shell, remote SQL, filesystem
customer impersonation, arbitrary commands
SIEM, risk score, severity engine, IP intelligence, geolocation, device reputation
incident management
automatic threat response
reliable clone detection
retention / purge / partitioning / archival
commercial_audit_event
```

## 21. `commercial_audit_event`

Fuera de C6 inicial. Representa auditoría del dominio comercial (actor = cuenta comercial), no mutaciones administrativas staff. No se mezcla con Audit C6. Puede existir una vista comercial independiente en el futuro.

## 22. Decisiones

### D-C6-01 — Audit vs Security separation

- **Decisión:** Audit = `staff_audit_event`. Security contiene dos datasets separados: `commercial_security_event` y `staff_auth_event`. Sin `UNION` físico ni pseudo-evento genérico. `commercial_audit_event` fuera.
- **Estado:** APROBADO.

### D-C6-02 — Read-only initial scope

- **Decisión:** C6 inicial es read-only; sin mutaciones ni respuesta automática.
- **Estado:** APROBADO.

### D-C6-03 — RBAC exacto

- **Decisión:** `audit.read` para Audit; `security.read` para Security (ambos datasets). `security.manage` reservada, sin superficie C6. Matriz runtime sin cambios.
- **Estado:** APROBADO.

### D-C6-04 — API paths

- **Decisión:** `/control/audit`, `/control/security-events`, `/control/staff-auth-events` (list/detail). Sólo `staffSession`.
- **Estado:** APROBADO.

### D-C6-05 — Pagination model

- **Decisión:** keyset descendente, `(timestamp DESC, id DESC)`, `limit 1..100`, cursor versionado con dataset y binding de filtros.
- **Estado:** APROBADO.

### D-C6-06 — Filters

- **Decisión:** los contratos de §11; sin búsqueda libre sobre JSONB ni trigram inicial.
- **Estado:** APROBADO.

### D-C6-07 — Metadata sanitization

- **Decisión:** whitelist por dataset/acción/kind, sólo en detail, fallback `{}`, nunca JSONB crudo.
- **Estado:** APROBADO.

### D-C6-08 — Append-only guarantees

- **Decisión:** mantener triggers; C6 no escribe; añadir tests; `TRUNCATE` queda como deuda de hardening.
- **Estado:** APROBADO.

### D-C6-09 — Retention policy status

- **Decisión:** `RETENTION POLICY: NO DEFINIDA`; C6 no implementa retención.
- **Estado:** APROBADO.

### D-C6-10 — Frontend information architecture

- **Decisión:** dos módulos top-level (Audit, Security); Security con tabs Platform events / Staff authentication; rutas de §14.
- **Estado:** APROBADO.

### D-C6-11 — Migration required

- **Decisión:** `0010` requerida, sólo índices (§12.1); no se crea en C6.0B.
- **Estado:** APROBADO.

### D-C6-12 — Out-of-scope boundaries

- **Decisión:** §20.
- **Estado:** APROBADO.

## 23. Implementation slicing

```
C6.0A  Inventario técnico                      COMPLETADO / VALIDADO
C6.0B  Contrato / ADR / RBAC / API / UX        este gate
C6.1   Backend read-side:
       - migration 0010 (índices)
       - repositories + keyset + filtros
       - DTOs + sanitización de metadata
       - Audit API + Security API + Staff Auth API
       - OpenAPI
       - tests backend
C6.2   Frontend:
       - Audit
       - Security platform events
       - Staff authentication view
       - client / query keys / filtros / list / detail
       - unit tests
C6.3   Hardening:
       - metadata sanitization tests
       - immutability tests
       - RBAC tests
       - performance / index validation
       - E2E × 5 viewports
       - security review
C6.4   Integration:
       - merge
       - CI main
       - closure documentation
```

## 24. Consecuencias

- C6.1 implementará la primera superficie de lectura de auditoría/seguridad.
- El contrato evita inventar severidad, riesgo, IP/UA o acciones no producidas.
- La separación por tabla evita perder semántica y permite evolucionar cada dominio por separado.
- La matriz de índices queda acotada a queries dominantes y evita duplicar índices existentes.
- `security.manage` y 4-eyes quedan explícitamente reservados, no eliminados.
- C6 sigue **NO IMPLEMENTADO** hasta completar C6.1–C6.4.
