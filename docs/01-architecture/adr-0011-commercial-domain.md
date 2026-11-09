# ADR-0011 — Dominio comercial: Plan, Subscription y Entitlement

**Estado:** PROPUESTO — pendiente de ratificación (Fase 1, 2026-11-09)
**Fase:** Beta 0.1.0 — Commercial Core
**Relacionado:** [Platform](../01-architecture/platform.md) · [ADR-0007](adr-0007-product-project-model.md) · [ADR-0009](adr-0009-pos-installation-control.md) · [Backlog Beta](../02-development/platform-beta-backlog.md) · [Seguridad Beta](../02-development/platform-beta-security.md)

> Este ADR propone el contrato comercial mínimo de Beta y queda pendiente de ratificación. No crea tablas ni endpoints en esta fase. El DDL definitivo se implementará en una migración posterior (`0011+`) tras revisión y en el PR correspondiente.

## 1. Contexto

Hoy existen:

- `product` y `project` (C2).
- `organization_product` con `status` y `entitlements` JSONB (C4), sólo lectura desde API.
- Un catálogo de planes **hardcodeado en Rust** (`PlanName`/`FeatureKey`) específico de POS.
- `issued_license` y un `LicenseGrant` v1 firmado con Ed25519, con emisión DEV-only.
- Ninguna tabla de suscripciones ni resolución efectiva de entitlements.

La Beta necesita un recorrido administrativo:

```text
Producto → Organización → OrganizationProduct → Plan → Subscription → Entitlements
```

sin convertir Platform en un sistema de facturación y sin romper la compatibilidad de licencias.

## 2. Decisiones

### D-BETA-COM-01 — Separación de conceptos

Se proponen como conceptos distintos:

| Concepto | No es | Autoridad |
|---|---|---|
| Product | Plan, precio, suscripción | Catálogo staff |
| Organization | Producto, suscripción | Dominio comercial |
| OrganizationProduct | Suscripción | Habilitación administrativa |
| Plan | Precio, factura, licencia | Catálogo comercial del producto |
| Subscription | Licencia, pago | Contrato comercial organización–producto–plan |
| Entitlement | Permiso staff, pago | Derecho efectivo resuelto desde suscripción + plan |
| License | Suscripción | Documento técnico firmado para una instalación |
| Installation | Usuario, licencia | Registro de instancia desplegada |

### D-BETA-COM-02 — Catálogo de planes por producto

- El catálogo de planes es **data**, no enum.
- Cada plan pertenece a un `product_id`.
- La clave del plan es única dentro del producto: `(product_id, key)`.
- Los planes `trial`, `local`, `pro` y `enterprise` se modelan como **datos iniciales del producto `pos`**, no como valores universales.
- Un producto nuevo puede definir sus propios planes, features y límites.
- No se impone un conjunto mínimo de planes a todos los productos.
- Un producto tiene como máximo **un plan `is_trial=true`**; un índice parcial único lo garantiza.
- Una organización puede contratar **varios productos**; cada par organización-producto tiene su propio plan y su propia suscripción. No existe un plan global compartido entre productos.

Modelo conceptual mínimo:

```text
plan
  id, product_id, key, name, description, status, is_trial, created_at, updated_at
  unique (product_id, key)
  status: draft | active | maintenance | deprecated | retired
```

- `draft`: en preparación; no asignable.
- `active`: asignable a nuevas suscripciones.
- `maintenance`: existente; no se asignan nuevas suscripciones salvo decisión explícita.
- `deprecated`: no se asignan nuevas; las existentes conservan vigencia.
- `retired`: no se asigna; se conserva por historial.

#### Inmutabilidad de las condiciones comerciales

- `key`, `product_id`, `is_trial`, features y límites son las **condiciones de uso** del plan.
- Mientras el plan está en `draft`, son editables.
- Un plan `active` sin ninguna subscription puede editarse con lock de la fila y auditoría; no hay términos contratados que se alteren.
- Cuando el plan tiene **cualquier subscription** (en cualquier estado), las condiciones de uso son **inmutables**. Cambiar derechos exige crear un **nuevo plan** con nueva `key` (mismo producto) y renovar/migrar la subscription de forma explícita.
- `name` y `description` son metadata no contractual y pueden editarse con auditoría.
- `status` puede transicionar de forma explícita; las subscriptions existentes conservan las condiciones del plan contratado hasta su expiración/cancelación.
- La inmutabilidad se valida en aplicación y, cuando corresponda, con trigger/constraint en la migración de B6.
- No se reescriben condiciones de un plan ya contratado ni se recalculan retroactivamente entitlements de subscriptions existentes.

Las features/límites del plan se modelan como entitlements declarativos del plan (tabla normalizada o estructura validada; el DDL se decide en el PR de implementación). No se usa JSONB arbitrario como autoridad.

### D-BETA-COM-03 — Subscription como agregado con historial

Modelo conceptual:

```text
subscription
  id, organization_id, product_id, plan_id
  status, is_trial, starts_at, ends_at
  created_at, updated_at, suspended_at?, cancelled_at?
  renewed_from_subscription_id?
  unique (organization_id, product_id) where status in ('active','suspended')
  unique (organization_id, product_id) where is_trial
```

```text
subscription_event   (append-only)
  id, subscription_id, event, from_status, to_status,
  occurred_at, actor_staff_account_id, reason, metadata saneada
  event: created | suspended | reactivated | cancelled | expired | renewed
```

Estados y transiciones:

| Estado | Significado | Transiciones permitidas |
|---|---|---|
| `active` | Vigente y con derechos efectivos | `suspended`, `cancelled`, `expired` |
| `suspended` | Vigente pero sin derechos efectivos | `active`, `cancelled`, `expired` |
| `cancelled` | Terminada por decisión administrativa | Terminal |
| `expired` | Terminada por vigencia | Terminal |

Reglas:

- `active ↔ suspended` es reversible y auditado.
- `cancelled` y `expired` son terminales; no se reactivan.
- Renovar crea una **nueva** subscription (`renewed_from_subscription_id`) y cierra la anterior (`expired` o `cancelled` con motivo). No se reescribe el historial.
- Sólo una subscription `active|suspended` por `(organization_id, product_id)`.
- La subscription debe referenciar un plan del mismo `product_id`.
- No se permite crear/activar una subscription si `organization_product.status != active`.
- No se modelan pagos, facturas, prorrateo, cupones ni trials automáticos.

#### Trial, suspensión, cancelación y expiración

- **Trial:** es un plan del producto con `is_trial=true` (snapshot en `subscription.is_trial`). Sólo se permite **un trial por `(organization_id, product_id)` durante toda la vida de la relación**; un índice parcial único lo garantiza. El trial no se convierte automáticamente en plan de pago ni renueva solo. `ends_at` es obligatorio.
- **Suspensión:** efecto inmediato; `effective=false` desde `suspended_at`; reversible a `active`; no cambia `starts_at`/`ends_at`.
- **Cancelación:** efecto inmediato; terminal; registra `cancelled_at`; no hay prorrateo, reembolso ni reactivación.
- **Expiración:** `now >= ends_at` ⇒ `effective=false`, `reason=expired`, aunque el `status` siga `active`. Una transición explícita `expire` actualiza `status='expired'` y escribe evento/auditoría. No se requiere job en Beta; el resolver aplica expiración lógica.
- **Renovación:** crea una **nueva** subscription con `renewed_from_subscription_id`; la anterior pasa a `cancelled` (motivo `renewed`) o `expired` si ya terminó, en la misma transacción. La nueva empieza en `now`; no hay solapamiento activo, prorrateo ni reembolso. Sólo se renueva desde `active` o `expired`; `suspended` debe reactivarse o cancelarse antes, y `cancelled` es terminal. `POST /subscriptions/{id}/renew` acepta `plan_id` opcional (mismo producto, plan `active`); si se omite, conserva el plan. Exige `Idempotency-Key`; mismo key + payload devuelve la misma renovación; payload distinto ⇒ `409 idempotency_conflict`.
- **Fechas:** UTC, `ends_at > starts_at`; vigencia `[starts_at, ends_at)`.
- **Idempotencia:** `POST /subscriptions` exige `Idempotency-Key`; mismo key + mismo payload devuelve la misma subscription; mismo key + payload distinto ⇒ `409 idempotency_conflict`.
- **Transacciones:** cada creación/transición bloquea la fila relevante (`organization_product` o `subscription`) `FOR UPDATE`, valida estado y escribe `subscription`, `subscription_event` y `staff_audit_event` en una sola transacción; un fallo revierte todo. El índice parcial único resuelve carreras.
- **Acciones de auditoría:** `plan_created`, `plan_updated`, `organization_product_activated`, `organization_product_deactivated`, `subscription_created`, `subscription_suspended`, `subscription_reactivated`, `subscription_cancelled`, `subscription_expired`, `subscription_renewed`. `subscription_event` es append-only (UPDATE/DELETE prohibidos).

### D-BETA-COM-04 — Resolver de entitlements

La autoridad comercial exige:

```text
Organization.status == active
  AND OrganizationProduct.status == active
  AND Subscription.status == active
  AND starts_at <= now < ends_at
  → Plan entitlements
```

Reglas de resolución:

| Condición | Resultado |
|---|---|
| Organization no existe o no está `active` | `effective=false`, `reason=organization_inactive`, sin derechos |
| OrganizationProduct no existe o `inactive` | `effective=false`, `reason=product_not_enabled`, sin derechos |
| Sin subscription | `effective=false`, `reason=no_subscription`, sin derechos |
| Subscription `suspended` | `effective=false`, `reason=suspended`; se devuelven las features del plan para contexto, sin concederlas |
| Subscription `cancelled` o `expired` | `effective=false`, `reason=terminated`, sin derechos |
| Subscription `active` pero `now < starts_at` | `effective=false`, `reason=not_started`, sin derechos |
| Subscription `active` pero `now >= ends_at` | `effective=false`, `reason=expired`, sin derechos (expiración lógica) |
| Subscription `active` y `now < ends_at` | `effective=true`; features/límites del plan |
| Plan `maintenance`/`deprecated`/`retired` en una subscription existente | Se conserva la subscription; no se asignan nuevas |

La expiración lógica (`now >= ends_at`) se evalúa antes que `suspended`/`cancelled`/`active`: si el periodo terminó, el resultado es `expired` y no concede derechos aunque el estado persistido no se haya actualizado.

- `organization_product.entitlements` **no es la autoridad** en Beta. Se conserva como metadata extensible/legacy; no se interpreta para conceder derechos sin una decisión posterior explícita.
- El resolver es una operación de lectura pura, sin side effects, y devuelve `effective`, `status`, `reason`, `plan`, `features` y `limits`.
- No se cachea en Beta; se podrá cachear cuando exista medición.
- La aplicación de derechos en productos es responsabilidad del producto; Platform expone el derecho efectivo.
- La expiración lógica por `ends_at` no requiere un job en Beta; el estado persistido puede seguir `active` hasta que un administrador ejecute `expire`.

### D-BETA-COM-05 — OrganizationProduct sigue siendo habilitación

- `organization_product` no es una suscripción ni un plan.
- `status: active|inactive` indica si el producto está habilitado para la organización.
- `activated_at` es inmutable y registra la primera activación.
- La desactivación no borra la fila ni el historial; no elimina suscripciones.
- La mutación de `organization_product` requiere `organizations.manage`, auditoría staff y ausencia de DELETE físico.
- Una suscripción no puede activarse si la asociación no está activa.

### D-BETA-COM-06 — Compatibilidad de licenciamiento v1

- `LicenseGrant` v1 y `issued_license` **no cambian** en Beta.
- El dominio comercial **no depende** de `installation` ni `issued_license`: un producto sin licencias offline o sin instalaciones puede tener plan, suscripción y entitlements sin emitir licencia.
- `issued_license.plan` sigue siendo TEXT; la migración a un `plan_id` no es requisito de Beta.
- La emisión productiva sigue deshabilitada por configuración (`ALLOW_DEV_ISSUANCE=false` por defecto, sin `devAdminToken` productivo).
- La suscripción es la base futura de elegibilidad, pero la emisión productiva requiere un ADR/implementación posterior con custodia de clave, elegibilidad, auditoría y compatibilidad POS.
- El **trial comercial** de Beta no reemplaza ni migra automáticamente el trial técnico legacy de POS (`installation.trial_started_at`/`trial_ends_at` y `POST /installations/{id}/trial`). Ambos coexisten; la integración POS definirá la autoridad final. No se modifica el endpoint legacy en Beta.
- No se transforma silenciosamente el documento de licencia v1.
- No se utiliza `devAdminToken` como mecanismo comercial.

### D-BETA-COM-07 — RBAC sin nuevas capabilities en Beta

Para evitar proliferación prematura, la Beta reutiliza capabilities existentes:

| Operación | Capability |
|---|---|
| Leer planes | `products.read` |
| Crear/editar planes | `products.manage` |
| Leer suscripciones | `subscriptions.read` |
| Crear/transicionar suscripciones | `subscriptions.manage` |
| Leer entitlements efectivos | `subscriptions.read` |
| Activar/desactivar OrganizationProduct | `organizations.manage` |
| Consultar auditoría comercial | `audit.read` |

- Se reutilizan `subscriptions.read`/`subscriptions.manage`, ya existentes en `platform-core` y ADR-0006. No se crean capabilities nuevas.
- No se crean `plans.manage` ni `entitlements.read` en Beta.
- Consecuencia: `operator`, `support`, `security_auditor` y `read_only` pueden **leer** planes, suscripciones y entitlements; sólo `super_admin` y `platform_admin` tienen `products.manage` y `subscriptions.manage`, y pueden crear/editar planes y transicionar suscripciones. Si una fase futura necesita delegación independiente, se añadirán capabilities explícitas con ADR.
- `security.read`/`audit.read` conservan la matriz ratificada; el riesgo se documenta en [Seguridad Beta](../02-development/platform-beta-security.md).
- `security_auditor` no obtiene `organizations.manage`, `subscriptions.manage` ni `installations.rehost`.

### D-BETA-COM-08 — API aditiva

Rutas propuestas (listado/detalle, sin DELETE físico):

```text
GET    /api/v1/control/plans?product_id=&status=&cursor=&limit=
POST   /api/v1/control/plans
GET    /api/v1/control/plans/{plan_id}
PATCH  /api/v1/control/plans/{plan_id}

GET    /api/v1/control/subscriptions?organization_id=&product_id=&status=&cursor=&limit=
POST   /api/v1/control/subscriptions
GET    /api/v1/control/subscriptions/{subscription_id}
POST   /api/v1/control/subscriptions/{subscription_id}/suspend
POST   /api/v1/control/subscriptions/{subscription_id}/reactivate
POST   /api/v1/control/subscriptions/{subscription_id}/cancel
POST   /api/v1/control/subscriptions/{subscription_id}/renew

GET    /api/v1/control/organizations/{organization_id}/products
POST   /api/v1/control/organizations/{organization_id}/products
PATCH  /api/v1/control/organizations/{organization_id}/products/{product_id}

GET    /api/v1/control/organizations/{organization_id}/entitlements?product_id=...
```

- Paginación keyset y errores normalizados, consistentes con C4/C5.
- `POST /subscriptions` exige `Idempotency-Key`; el reintento con el mismo key y payload devuelve la misma subscription; payload distinto ⇒ `409 idempotency_conflict`.
- `POST /plans` y `PATCH /plans/{id}` validan `(product_id, key)` único; duplicado ⇒ `409`.
- `POST /organizations/{id}/products` es idempotente por PK y sólo audita si cambia el estado; `PATCH` cambia `active|inactive` con auditoría.
- `409` para transiciones inválidas, plan de otro producto, organización-producto inactivo, trial duplicado o suscripción solapada.
- `404` neutral para recursos inexistentes.
- No `DELETE`; no `PATCH` genérico de estado de subscription; las transiciones son POST explícitos.

### D-BETA-COM-09 — Orden de migraciones

- `0010`: índices de lectura de C6 (ADR-0010 / PR #22), implementado en C6.1.
- `0011+`: dominio comercial.
- Si negocio exige comercial antes que C6, C6 se renumera a `0011` y ADR-0010 se actualiza antes de implementar.
- No se reescriben `0001–0009`.
- El seed de planes POS se hace como datos del producto `pos`, no como enum universal.

### D-BETA-COM-10 — Validación con segundo producto

- La Beta debe validar el dominio comercial con un producto sintético distinto de POS.
- Ese producto puede no requerir instalaciones, licencias offline ni enrolamiento.
- La validación debe demostrar que el catálogo, la suscripción y los entitlements funcionan sin nombres de plan POS.

### D-BETA-COM-11 — Customer Portal comercial read-only

El cliente autenticado debe poder consultar sus productos contratados, plan vigente, estado de suscripción y derechos comerciales relevantes, sin acceso a superficies staff ni a otras organizaciones.

API comercial (no `/control`):

```text
GET /api/v1/organizations/{organization_id}/products
GET /api/v1/organizations/{organization_id}/products/{product_id}
```

- Autenticación: `commercialSession` + membresía activa en la organización.
- Autorización por organización en SQL; organización ajena o inexistente ⇒ `404` neutral.
- Read-only: sin checkout, sin cambios de plan, sin cancelación, sin métodos de pago, sin endpoints staff.
- Filtro: sólo productos con `organization_product.status='active'`; un producto sin suscripción se muestra con `subscription=null` y `effective=false`, `reason=no_subscription`.
- Respuesta por producto: `product { id, key, name }`, `organization_product { status }`, `subscription { id, status, plan { id, key, name }, starts_at, ends_at, is_trial } | null`, `entitlements { effective, reason, features, limits }`.
- No se exponen datos de otras organizaciones, actores staff, auditoría interna, `issued_license` ni secretos.
- No se implementan pagos, facturación ni autoservicio de cambios comerciales.

## 3. Alternativas descartadas

- **Reutilizar `PlanName` como catálogo universal.** Descartada: acopla el núcleo a POS y bloquea productos con otros niveles.
- **Usar `organization_product.entitlements` como autoridad.** Descartada: JSONB sin contrato, sin vigencia, sin historial y sin separación de decisión comercial.
- **Crear una subscription dentro de `organization_product`.** Descartada: mezcla habilitación, plan y contrato; impide historial y estados.
- **Modelar pagos/facturas en Beta.** Descartada: fuera de alcance.
- **Emitir licencias productivas desde suscripciones en Beta.** Descartada por custodia de clave y compatibilidad POS; se prepara el contrato.

## 4. Consecuencias

- Nuevas tablas y servicios en `0011+`; sin cambios en C1–C5.
- Control Center incorpora módulos de Planes y Suscripciones, y mutaciones de OrganizationProduct.
- El Customer Portal debe mostrar productos contratados, plan, estado de suscripción y entitlements efectivos en modo read-only (D-BETA-COM-11).
- `LicenseGrant` v1 permanece intacto; la integración POS se hará en Fase 8.
- La resolución de entitlements es un read model; no se implementa enforcement en productos dentro de Platform Beta.
- La deuda de pagos/facturación queda explícitamente fuera.

## 5. Criterios de aceptación del dominio comercial

1. Un producto genérico puede tener su propio catálogo de planes.
2. Una organización puede tener un producto habilitado y una suscripción activa.
3. Una suscripción sólo puede usar un plan del mismo producto.
4. Suspender/reactivar/cancelar/renovar producen transiciones auditadas.
5. El resolver devuelve derechos efectivos coherentes con el estado.
6. Desactivar OrganizationProduct impide derechos efectivos.
7. No se puede crear una segunda suscripción activa para el mismo par organización-producto.
8. El historial de suscripción es append-only.
9. El contrato de licencia v1 no cambia.
10. Un producto sin instalaciones puede operar el flujo comercial sin exigirlas.
11. Una organización con dos productos mantiene dos suscripciones independientes.
12. Un trial no puede repetirse para el mismo par organización-producto.
13. Una suscripción expirada lógicamente no concede derechos aunque su `status` persistido siga `active`.
14. Un fallo de transacción no deja subscription, evento ni auditoría parciales.
15. Las condiciones de uso de un plan con subscriptions son inmutables; un cambio de derechos exige un plan nuevo y renovación/migración explícita.
16. Una renovación reintentada con la misma `Idempotency-Key` no duplica la subscription.
17. El Customer Portal muestra productos contratados, plan, suscripción y entitlements de la organización autenticada.
18. El Customer Portal no expone endpoints staff, datos de otra organización ni `issued_license`.
