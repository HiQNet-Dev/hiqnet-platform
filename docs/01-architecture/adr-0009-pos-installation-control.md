# ADR-0009 — Control administrativo de instalaciones POS

**Estado:** C5.0B APROBADO; C5.0C APROBADO; C5.1/C5.2 implementados y validados localmente. C5 no integrado en `main`.
**Fecha:** 2026-11-09
**Fase:** C5 — POS Control
**Relacionado:** [ADR-0003](adr-0003-installation-enrollment.md), [ADR-0004](adr-0004-control-center-boundaries.md), [ADR-0007](adr-0007-product-project-model.md), [ADR-0008](adr-0008-control-center-frontend.md)

> Este documento describe el contrato ratificado. La migración `0009` está implementada en la rama C5 y publicada en el PR #20, pero aún NO está integrada en `main`.

## 1. Definición de Installation (D-C5-01)

Una **Installation** es el registro lógico y persistente de una instancia desplegada de un Product HiQNet en un dispositivo concreto, perteneciente a una Organization.

No es Organization, Branch, caja lógica, empleado, usuario, licencia ni suscripción.

Invariantes:

- `installation_id` es identidad lógica estable y no cambia por rotación de clave, reinstalación o rehost.
- La Installation conserva historial de identidades, bindings, licencias y eventos.
- La Installation no cambia silenciosamente de Organization.
- Una keypair no es una Installation: la identidad criptográfica es una generación asociada.
- `installation.status`, `identity_status`, `installation_identity`, `installation_binding` y `enrollment_request` son ejes separados.

**Estado: APROBADA.**

## 2. Product por Installation (D-C5-02)

Cardinalidad objetivo:

```text
Organization 1 ─── N Installation
Product      1 ─── N Installation
Installation N ─── 1 Organization
Installation N ─── 1 Product
```

### 2.1 Por qué no derivar Product sólo desde `organization_product`

Una organización puede tener varios `organization_product` (por ejemplo HiQNet POS y otro producto). La asociación organización↔producto no identifica qué producto ejecuta una instalación concreta. La atribución debe ser explícita por Installation.

### 2.2 Regla para instalaciones nuevas

Toda Installation nueva debe quedar atribuida a exactamente un Product **antes de completar Enrollment**.

```text
POS inicia enrollment
      ↓
declara identidad estable del Product
      ↓
Platform resuelve ese Product
      ↓
owner comercial aprueba
      ↓
Platform valida organization_product(organization_id, product_id)
con status = active
      ↓
POS realiza proof
      ↓
Installation se crea/actualiza con product_id
```

Una Installation nueva **no puede completarse** si:

- el Product no existe;
- no existe `organization_product`;
- `organization_product.status != active`;
- el Product resulta ambiguo.

Debe fallar antes de persistir una atribución inválida.

La representación externa exacta (`product_key` vs `product_id`) pertenece a C5.0C. **Recomendación no ratificada:** preferir `product.key` como identidad externa estable y resolver internamente a UUID.

### 2.3 Asociación que pasa a `inactive` después

Si posteriormente `organization_product.status = inactive`:

- `installation.product_id` **no cambia**;
- `installation.status` **no cambia automáticamente**;
- la asociación se conserva como histórico.

No se propaga el lifecycle de `organization_product` a la Installation.

### 2.4 Backfill legacy (problema distinto de la atribución nueva)

En el repositorio no existe regla inequívoca:

- no hay `installation.product_id`;
- no hay seed canónico de producto POS;
- la clave `pos` sólo aparece en tests;
- `organization_product` no se crea para instalaciones existentes.

```text
Instalaciones nuevas:  NO se permiten sin product_id inequívoco.
Instalaciones legacy:  pueden conservar product_id NULL durante una transición controlada.
```

Estrategia:

```text
FASE A: product_id NULL + integridad relacional + backfill explícito por organización
FASE B: validar cero NULL y revisar ambigüedades
FASE C: NOT NULL cuando exista política de atribución aprobada
```

Integridad objetivo futura: FK compuesta `(organization_id, product_id) → organization_product(organization_id, product_id)`. No se crea SQL todavía.

**Estado: APROBADA** (regla para nuevas, estrategia de backfill). Migración **NO AUTORIZADA**.

## 3. Identidad lógica vs criptográfica (D-C5-03)

```text
installation_id ≠ public_key ≠ fingerprint ≠ installation_identity.id
```

```text
Installation
  ├── installation_id estable
  └── N InstallationIdentity
          generation 1
          generation 2
          generation 3
```

- Máximo una identidad activa por instalación.
- Máximo un binding activo por instalación.
- Rotación/rehost conserva `installation_id` y añade generación.

**Estado: APROBADA.**

## 4. Rehost (D-C5-04)

### 4.1 Escenarios

| Escenario | Comportamiento |
|---|---|
| Reinstalación de app, mismo SO y clave | No cambia identidad ni binding si conserva keypair e `installation_id`. |
| Reinstalación de SO / cambio de SSD | Requiere prueba de posesión con la misma clave o rehost si se perdió. |
| Restauración de backup SQLite | Sólo válida si conserva la clave privada y no hay rehost en curso. |
| Clonación accidental de SQLite | Una sola identidad/binding activos; no se puede crear un binding paralelo. |
| Pérdida de private key / cambio de equipo / reemplazo de caja | **Rehost**: misma Installation, nueva generación de identidad y nuevo binding. |

### 4.2 Cutover atómico

La autorización de rehost y la invalidación de la identidad anterior forman **una única unidad transaccional conceptual**:

```text
BEGIN
  persistir autorización de rehost (installation_id, actor, motivo,
                                   created_at, expires_at, usable)
  retirar identity generation N (retired_at)
  revocar binding activo (revoked_at + reason)
  installation.identity_status = unenrolled
  staff audit
  security event
COMMIT
```

Si cualquier paso falla: **ROLLBACK**. Es obligatorio. Nunca se permite retirar la identidad y confirmar antes de persistir la autorización, porque dejaría la Installation bloqueada sin recovery válido.

### 4.3 Requisitos de la autorización

La persistencia propuesta se detalla en C5.0C; debe ser:

- persistida;
- one-time;
- expirable;
- atada a `installation_id`;
- atada al staff actor;
- con motivo;
- no reutilizable tras completion;
- evaluar snapshot/invariante de `organization_id` y `product_id`.

### 4.4 Si el rehost no termina

La Installation queda con `identity_status = unenrolled`, sin binding activo. La clave anterior no revive automáticamente. Se requiere nueva autorización (o renovación explícita si C5.0C lo decide). No hay rollback automático a una identidad previa.

**Resuelto en C5.1:** una nueva autorización es posible desde `unenrolled` únicamente si la Installation ya estuvo enrolada (`enrolled_at` no nulo) y no conserva identidad ni binding activos. Así el reintento no queda bloqueado y un estado inconsistente (`unenrolled` con identidad activa) se rechaza con `409`.

### 4.5 Clave anterior

Una generación retirada no puede volver a activarse. Cualquier proof posterior con esa clave debe fallar, con independencia de que el dispositivo físico siga existiendo.

### 4.6 Dos claves nuevas en el mismo rehost

Sólo la primera completion válida consume la autorización: crea la generación N+1, el binding activo y marca la autorización como usada. Intentos posteriores reciben `409 Conflict`. Los índices parciales de identidad/binding activos son defensa adicional en PostgreSQL.

### 4.7 Limitación de detección de clones

Si dos dispositivos tienen el mismo `installation_id` y la misma private key, Platform **no puede distinguir criptográficamente** cuál es el original usando sólo la identidad actual. `clone_suspected` puede existir como evento, pero C5 no promete detección fiable de clones. Una detección robusta requeriría attestation de hardware, almacenamiento ligado al dispositivo u otra señal de confianza, fuera de C5.

### 4.8 El POS anterior no se borra ni se apaga

Retirar una identidad en Platform sólo invalida su relación criptográfica futura con Platform. No implica borrar SQLite, detener ventas locales, bloquear lectura, apagar la aplicación ni eliminar datos. El comportamiento offline/licensing del dispositivo anterior pertenece a reglas futuras de lease/licensing y no se define en C5.

**Estado: APROBADA** (semántica y cutover). Persistencia detallada en C5.0C; RBAC de rehost ratificado en C5.0C.

## 5. Lifecycle administrativo (D-C5-05)

Se conserva `installation.status`:

- **active:** Installation administrativamente habilitada en Platform. No garantiza online, licencia vigente, suscripción vigente, sync saludable ni heartbeat reciente.
- **suspended:** Installation administrativamente suspendida por Platform. Reversible, conserva historial e identidad/binding salvo operación de seguridad separada y no borra datos locales. El efecto futuro sobre operaciones POS queda fuera de C5 inicial.
- **unregistered:** LEGACY / COMPATIBILITY. No debe producirse para nuevas Installations en C5. No se elimina por datos/rutas históricas.

No se añade `revoked` ni `replaced`.

### 5.1 Normalización legacy

Si una Installation legacy con `status = unregistered` e `identity_status = unenrolled` completa un Enrollment válido, Platform normaliza:

```text
installation.status = active
identity_status = active
```

Implementado en C5.1 y cubierto por tests; evita producir `unregistered + active`.

**Estado: APROBADA.**

## 6. Lifecycle criptográfico (D-C5-05B)

### 6.1 Nivel generación

Cada fila de `installation_identity` puede terminar con `retired_at != NULL`: esa generación concreta deja de ser válida. Una Installation puede tener generaciones 1..N retiradas y exactamente una activa.

### 6.2 Nivel aggregate

`installation.identity_status` representa el estado criptográfico global:

- **unenrolled:** actualmente no existe identidad ni binding activos; la Installation puede completar enrollment/rehost.
- **active:** existe exactamente una identidad activa y un binding activo correspondientes.
- **retired:** la identidad criptográfica de esa Installation fue retirada de manera **terminal**. No vuelve a `active` dentro de C5. Cualquier “unretire” futuro exigirá decisión/ADR específica.

### 6.3 Máquina de estados

```text
unenrolled ── enrollment completed / valid proof ──▶ active
active     ── authorize rehost ────────────────────▶ unenrolled
unenrolled ── new valid proof ─────────────────────▶ active
active     ── permanent retirement ────────────────▶ retired
```

Prohibidas:

```text
retired → active
retired → unenrolled
unenrolled → retired sin operación explícita
active → active por cambio silencioso de key
```

Durante rehost: la generación N pasa a `retired_at`, pero el aggregate va `active → unenrolled → active`; nunca `active → retired → active`.

**Estado: APROBADA.**

## 7. DELETE físico (D-C5-06)

No se permite DELETE de `installation`: auditoría, licencias, generaciones de identidad, bindings, eventos de seguridad, sync futuro y forense. Debe protegerse con trigger PostgreSQL en la migración futura.

**Estado: APROBADA.**

## 8. Conectividad (D-C5-07)

`last_seen_at` no significa presencia online. C5 no crea heartbeat, no calcula online/offline y no muestra conectividad inferida. Se expone como “última actividad conocida”.

**Estado: APROBADA.**

## 9. Ownership del enrollment (D-C5-08)

Flujo ordinario sin cambios:

```text
POS inicia
commercial owner aprueba
POS proof
Platform completa
```

`staffSession` no sustituye al owner y `installations.manage` no permite aprobación ordinaria de enrollment. Excepciones (`legacy_unclaimed`, soporte, recovery especial) quedan PENDIENTES y fuera de C5 inicial, salvo rehost como operación administrativa separada.

**Estado: APROBADA** (flujo ordinario).

## 10. Licensing boundary (D-C5-09)

C5 queda completamente fuera de licensing, incluida la lectura: trial, licencias emitidas, planes, emisión, renovación, lease, suscripción, billing y pricing. El frontend C5 no usará `/api/v1/installations/{id}/trial` ni `/licenses`.

**Estado: APROBADA.**

## 11. Sync boundary (D-C5-10)

C5 no implementa outbox, inbox, sync state, conflictos, force sync, retries ni replicación. No se añaden campos falsos de sync a Installation.

**Estado: APROBADA.**

## 12. Remote-control boundary (D-C5-11)

Fuera de C5: shell, SQL console, filesystem, remote desktop, comandos arbitrarios, database reset, remote delete e impersonation. Sin command bus genérico.

**Estado: APROBADA.**

## 13. `devAdminToken` — LEGACY / DEV

| Endpoint | Clasificación |
|---|---|
| `POST /api/v1/installations` | LEGACY / DEV |
| `GET /api/v1/installations/{id}` | LEGACY / DEV |
| `POST /api/v1/installations/{id}/trial` | LEGACY / DEV |
| `GET /api/v1/installations/{id}/licenses` | LEGACY / DEV |
| `POST /api/v1/installations/{id}/licenses` | LEGACY / DEV |

Se mantienen sin cambios. No se consumen desde Control Center, no se modifica el security scheme y no se convierten en mecanismo de producción. Sustitución y deprecación futuras explícitas.

## 14. Auditoría

Toda mutación staff futura registra en `staff_audit_event`, atómicamente con el cambio: `staff_account_id`, `installation_id`, `organization_id`, `product_id` cuando exista, timestamp, acción, `before`, `after`, `reason` y `result`. Nunca private keys, tokens, device/user codes crudos, nonces ni contraseñas.

Acciones candidatas (nombres finales en C5.0C):

```text
installation_rehost_authorized
installation_identity_retired
installation_binding_revoked
installation_rehost_completed
installation_suspended
installation_reactivated
```

## 15. Matrices

### 15.1 Administrativa

| FROM | Acción | TO | Permitida |
|---|---|---|---|
| active | suspend | suspended | Sí |
| suspended | reactivate | active | Sí |
| unregistered | enrollment complete | active | Sí (normalización legacy) |
| active | unregister | unregistered | No |
| suspended | unregister | unregistered | No |
| cualquiera | delete | — | No |

### 15.2 Identidad (aggregate)

Ver sección 6.3. `retired` es terminal en C5.

### 15.3 Compatibilidad

| installation.status | identity_status | binding activo | Clasificación | Motivo |
|---|---|---|---|---|
| active | active | sí | VALID | operativo normal |
| suspended | active | sí | VALID | suspensión conserva identidad |
| active | unenrolled | no | TRANSITIONAL | enrollment/rehost pendiente |
| suspended | unenrolled | no | TRANSITIONAL | recovery con suspensión |
| unregistered | unenrolled | no | LEGACY | pre-enrollment / compatibilidad |
| unregistered | active | sí | LEGACY-INCONSISTENT | histórico posible; no producir nuevo |
| active | retired | no | INVALID | admin active con identidad terminal |
| suspended | retired | no | VALID | retirada terminal y suspensión admin |
| unregistered | retired | no | LEGACY | histórico terminal |
| cualquiera | active | no | INVALID | identity activa sin binding |
| cualquiera | unenrolled | sí | INVALID | binding activo sin identidad activa |
| cualquiera | retired | sí | INVALID | binding activo hacia identidad retirada |

## 16. Invariantes

### 16.1 Identidad

- `identity_status = active` ⇒ exactamente una `installation_identity` con `retired_at IS NULL` y exactamente un `installation_binding` con `revoked_at IS NULL`, correspondientes entre sí.
- `identity_status = unenrolled` ⇒ cero identidades activas y cero bindings activos; puede haber generaciones históricas retiradas.
- `identity_status = retired` ⇒ cero identidades activas y cero bindings activos; retiro terminal del aggregate.

### 16.2 Ya protegidas por PostgreSQL

Una identidad activa por instalación; un binding activo por instalación; fingerprint activo global único; generación única por instalación; binding en la misma organización que la instalación; FKs `RESTRICT`; auditoría y eventos de seguridad append-only.

### 16.3 Propuestas futuras DB

- Installation sin DELETE (trigger).
- FK compuesta `(organization_id, product_id) → organization_product`.
- `organization_id` inmutable.
- `product_id` inmutable una vez atribuido.
- Consistencia completa `identity_status ↔ identidad activa ↔ binding activo`: puede requerir trigger o parte transaccional en application service; no forzar constraints imposibles.

### 16.4 Application service

Autorización; matriz de transiciones; atomicidad del rehost; consumo one-time de autorización; motivo; idempotencia; no reutilización de claves retiradas; atribución de Product antes de completar; auditoría atómica con la mutación.

## 17. Riesgos y pendientes

- Persistencia de la autorización de rehost.
- RBAC exacto de rehost.
- Representación externa del Product en enrollment (`product_key` vs `product_id`).
- Backfill histórico de Product.
- Comportamiento POS ante `suspended`.
- Detección robusta de clones (fuera de C5).
- Deprecación de `unregistered` y de `devAdminToken`.

## 18. Matriz final de decisiones

| Decisión | Estado |
|---|---|
| D-C5-01 Installation definition | APROBADA |
| D-C5-02 Product/cardinality | APROBADA — atribución obligatoria en nuevas; backfill legacy por fases |
| D-C5-03 Identity | APROBADA |
| D-C5-04 Rehost semantics | APROBADA — persistencia propuesta y RBAC ratificado en C5.0C |
| D-C5-05 Administrative lifecycle | APROBADA |
| D-C5-05B Identity lifecycle | APROBADA — `retired` terminal |
| D-C5-06 Delete | APROBADA |
| D-C5-07 Connectivity | APROBADA |
| D-C5-08 Enrollment ownership | APROBADA (flujo ordinario); excepciones fuera de C5 inicial |
| D-C5-09 Licensing boundary | APROBADA |
| D-C5-10 Sync boundary | APROBADA |
| D-C5-11 Remote-control boundary | APROBADA |

## 19. Estado del gate C5.0B

```text
C5.0B: APROBADO
C5.0C: APROBADO
C5.1: IMPLEMENTADO EN RAMA / VALIDADO LOCALMENTE + CI REMOTO
C5.2: IMPLEMENTADO EN RAMA / VALIDADO LOCALMENTE + CI REMOTO
0009: IMPLEMENTADA EN RAMA / NO integrada en main
```

---

# C5.0C — API / Security / Persistence / UX Contract

## C1. Modelo de persistencia de rehost

Rehost es un flujo de **doble autoridad**: el owner comercial aprueba el Enrollment y staff autoriza el cutover de identidad. Staff no sustituye ni falsifica la aprobación comercial.

```text
nuevo dispositivo
→ begin enrollment (mismo installation_id, product_key, nueva public key)
→ EnrollmentRequest R (pending)
→ owner comercial aprueba R
→ R = approved (enrollment_request_id)
→ staff con installations.rehost autoriza R
→ cutover atómico: retirar identidad N, revocar binding, identity_status=unenrolled
→ proof de R
→ generation N+1 + nuevo binding + identity_status=active
```

Nueva entidad propuesta:

```text
installation_rehost_authorization
  id UUID PK
  installation_id UUID NOT NULL FK installation RESTRICT
  enrollment_request_id UUID NOT NULL UNIQUE FK enrollment_request RESTRICT
  organization_id UUID NOT NULL          -- snapshot
  product_id UUID NULL                    -- snapshot; NULL solo si installation legacy sin atribución
  authorized_by_staff_account_id UUID NOT NULL FK staff_account RESTRICT
  reason TEXT NOT NULL                    -- 1..500 caracteres
  status TEXT NOT NULL                    -- pending | consumed | expired | cancelled
  created_at TIMESTAMPTZ NOT NULL
  expires_at TIMESTAMPTZ NOT NULL
  consumed_at TIMESTAMPTZ
  cancelled_at TIMESTAMPTZ
```

Reglas:

- La autorización está ligada a un **EnrollmentRequest concreto**; no es una ventana genérica por Installation. Ningún request competidor puede aprovecharla.
- `expires_at > created_at` y `expires_at <= enrollment_request.expires_at`. Ambas expiraciones deben caducar juntas; no se mantiene un challenge criptográfico 24 h.
- Máximo **una autorización utilizable** por Installation: índice único parcial `(installation_id) WHERE status = 'pending'`.
- `consumed`, `expired` y `cancelled` son terminales y nunca reutilizables; un nuevo intento exige un nuevo EnrollmentRequest.
- Consumo one-time en la misma transacción del proof.
- Expiración no sólo perezosa al consumir: al crear una nueva autorización, el servicio debe bloquear las existentes, marcar `expired` cualquier `pending` vencida y después insertar la nueva. El índice parcial no debe bloquear una autorización legítima por una pending vencida.
- `organization_id`/`product_id` son snapshot y deben coincidir con la Installation y el request al consumir.
- `reason` obligatoria y auditada; sin secretos.

Alternativa evaluada: reutilizar `enrollment_request`. Se rechaza como sustituto del lifecycle comercial; se usa como **referencia obligatoria** del rehost.

Transiciones:

```text
pending → consumed   (proof válido, transacción de completion)
pending → expired    (evaluación al consumir o al crear una nueva autorización)
pending → cancelled  (cancelación administrativa explícita)
```

Nunca: `consumed/expired/cancelled → pending`. No se define TTL independiente de 24 h para C5 inicial.

## C2. RBAC de rehost

Comparación:

| Opción | Ventaja | Problema |
|---|---|---|
| A. `installations.manage` | simple, sin nueva capability | `operator` ya posee manage; rehost rota credenciales y no debería quedar al mismo nivel que suspender |
| B. `installations.manage` + `security.manage` | eleva el listón | intersección actual deja sólo `super_admin`; mezcla dominios y puede bloquear operación legítima |
| C. nueva `installations.rehost` | least privilege explícito y auditable | requiere actualizar ADR-0006 y la matriz RBAC |

**Propuesta: C — capability `installations.rehost`.**

- `suspend`/`reactivate`: `installations.manage`.
- `rehost authorize`: `installations.rehost`.
- Mapeo propuesto: `super_admin` y `platform_admin` la reciben; `operator`, `support`, `security_auditor` y `read_only` no.
- No se decide por rol en código; el servicio comprueba la capability.

Mapeo **ratificado para C5 inicial**: `super_admin ✓`, `platform_admin ✓`, `operator —`, `support —`, `security_auditor —`, `read_only —`. No se usa `security.manage` para rehost, porque convertiría al auditor de seguridad en operador de identidad de clientes. ADR-0006 y la matriz RBAC se actualizarán al versionar C5.

## C3. Contrato de Product en Enrollment

Wire contract propuesto:

```text
POS envía: product_key
Platform resuelve: product.id (UUID interno)
```

Justificación: `product.key` es la identidad externa estable y evita acoplar el cliente a UUID internos.

Reglas:

- El request hash y la idempotencia se calculan sobre el payload canónico con `product_key`.
- Platform resuelve el Product y persiste `product_id` resuelto, nunca un UUID confiado sin validación.
- En `begin`: el Product debe existir. Para una Installation **nueva** se exige `Product.status = active`; una allowlist, no `status != retired`.
- **Consistencia Installation↔Product:** si la Installation ya tiene `product_id = Some(P)`, el Product resuelto debe ser exactamente `P` en `begin` (fail early), `approve` (revalidación) y `proof` (autoridad transaccional final). Un Product distinto se rechaza con error neutral, aunque esté `active` y asociado a la organización.
- **Legacy `product_id IS NULL`:** un Enrollment válido realiza el write-once `NULL → resolved product_id` en el proof; no se atribuye nada a filas sin Enrollment válido.
- Para rehost de una Installation existente se permite `Product.status ∈ {active, maintenance}`. `deprecated` queda fuera salvo decisión explícita futura.
- La asociación `organization_product` se valida en aprobación y proof, cuando ya existe organización, y debe estar `active`.
- En aprobación y proof: `organization_product(organization_id, product_id)` debe existir y estar `active`. Aprobación revalida además el estado del Product con la misma allowlist (nueva: `active`; rehost: `active|maintenance`); el proof es la autoridad final por TOCTOU.
- Si la asociación deja de estar activa entre begin y proof, el enrollment falla de forma neutral (`409 enrollment_invalid`) sin persistir atribución inválida.
- `enrollment_request.product_id` pasa a formar parte del modelo; para registros legacy puede quedar NULL.

Cambio de Product entre begin y completion no se permite: el `product_id` resuelto en begin es el que se valida y persiste.

## C4. Atribución legacy y backfill

Procedimiento propuesto (no ejecutado):

1. Reporte de instalaciones con `product_id IS NULL`, agrupadas por organización.
2. Para cada organización:
   - exactamente un Product activo en `organization_product`: candidato inequívoco para revisión;
   - varios activos: **ambiguo**, requiere elección explícita;
   - ninguno activo: **sin asignación**, no inventar;
   - sólo `inactive`: **sin asignación automática**;
   - `legacy_unclaimed`: permanece NULL hasta reclamación.
3. La atribución se ejecuta con un procedimiento explícito y auditado, nunca por heurística automática en la migración.
4. Fase B: verificar cero NULL donde exista política de atribución aprobada.
5. Fase C: `NOT NULL` sólo tras validación y decisión explícita.

C5 puede mostrar `product: null` como **“Unassigned”** para legacy, sin inventar datos.

## C5. Read models

```text
ControlInstallationSummary
  id, device_label
  organization { id, display_name, status }
  product { id, key, name } | null
  administrative_status
  identity_status
  registered_at, enrolled_at
  last_known_activity_at
  identity_generation
  binding_active (bool)
  fingerprint

ControlInstallationDetail
  summary
  current_identity { generation, fingerprint, created_at } | null
  current_binding { bound_at, approved_by_account_id } | null
  rehost { status, expires_at, reason } | null  -- sólo autorización pending actual
```

- El detalle C5 **no expone un objeto Enrollment completo**. El Enrollment comercial sigue siendo la autoridad del flujo, pero su read model e histórico no forman parte de este endpoint C5.
- `rehost` representa únicamente la autorización `pending` actual. Una autorización `consumed` se conserva en PostgreSQL como histórico, pero no se devuelve como estado actual del detalle.

- `fingerprint`: hex canónico completo del fingerprint (no la public key completa). La UI puede mostrarlo truncado y permitir copiar el valor completo.
- El detalle C5 **no carga históricos ilimitados** de identidades, bindings, enrollments ni autorizaciones. Esos históricos permanecen en PostgreSQL y se difieren a C6 Audit/Security o a endpoints paginados futuros.
- Sin licencias, trial, suscripción, billing, sync health, online/offline, CPU/RAM, IP, geolocalización ni comandos remotos.
- `product` puede ser null sólo en legacy; la UI lo etiqueta “Unassigned”.

## C6. Endpoints

```text
GET  /api/v1/control/installations
GET  /api/v1/control/installations/{installation_id}
POST /api/v1/control/installations/{installation_id}/suspend
POST /api/v1/control/installations/{installation_id}/reactivate
POST /api/v1/control/installations/{installation_id}/rehost
```

- No DELETE. No PATCH genérico de estado.
- Listado: `status`, `identity_status`, `organization_id`, `product_id`, `search` (device_label), `cursor`, `limit` (1..100), keyset `(registered_at, id)`.
- `GET` detalle: `404` neutral.
- `suspend`/`reactivate`: `installations.manage`.
- `rehost`: `installations.rehost` (ratificado para C5 inicial).
- `POST .../rehost` recibe `{ enrollment_request_id, reason }`; no acepta una ventana genérica por Installation.
- La completion de rehost **no** es un endpoint staff: ocurre en el proof del EnrollmentRequest autorizado.

## C7. Matriz de transiciones y mutaciones

| Acción | FROM | TO | Precondición | Resultado |
|---|---|---|---|---|
| suspend | administrative active | suspended | sin cambio de identidad | 200 |
| reactivate | suspended | active | — | 200 |
| rehost authorize | administrative active o suspended | mismo estado administrativo (identity unenrolled) | EnrollmentRequest aprobado del mismo installation_id; `identity_status=active` con exactly una identidad y un binding activos, **o** `identity_status=unenrolled` con `enrolled_at` no nulo y cero identidad/binding activos (reintento); sin autorización pending; Product allowlist | 200 + autorización |
| rehost completion | identity unenrolled + autorización pending vigente | identity active | product/organization snapshot coinciden; proof válido | 200/201 enrollment completion |

Idempotencia propuesta:

- `suspend` sobre `suspended` y `reactivate` sobre `active`: **200 idempotente**, sin nueva auditoría si no hubo cambio.
- `unregistered`: suspend/reactivate → `409 transition_conflict`.
- rehost con autorización pending existente → `409 rehost_conflict`.
- rehost sin identidad/binding activos, con `unregistered` o con `identity_status=retired` → `409 transition_conflict`.
- rehost conserva el estado administrativo: `active → active`, `suspended → suspended`; nunca cambia a `active` por sí solo.
- **Retry seguro por identidad natural:** mismo `installation_id` + mismo `enrollment_request_id` devuelve la misma autorización/resultado, no repite el cutover ni duplica auditoría. Otro `enrollment_request_id` mientras existe `pending` → `409 rehost_conflict`.
- Un request cuyo EnrollmentRequest no pertenece a la Installation, no está `approved` o no tiene owner approval vigente → `409 rehost_conflict`.
- completion sin autorización vigente → `409 enrollment_invalid` neutral.
- segunda completion → `409`; la autorización ya está consumida.

Cada mutación exitosa genera auditoría staff y, cuando aplique, evento de seguridad, atómicamente.

## C8. Semántica de errores

- `400 validation_error`: query, body, UUID, límites.
- `401`: sesión staff ausente/expirada.
- `403 forbidden`: capability insuficiente.
- `404 not_found`: Installation inexistente; neutral.
- `409 transition_conflict`: transición no permitida.
- `409 rehost_conflict`: autorización pending, completion duplicada o estado de identidad incompatible.
- `409 rehost_expired`: autorización expirada al intentar consumir.
- `409 product_unavailable`: Product inexistente, fuera de allowlist o `organization_product` no activa; mensaje neutral sin filtrar tenants.
- `503 storage_error`.

Estos códigos aplican a la superficie `/api/v1/control/*`. El endpoint público de proof no revela internals de Control: autorización de rehost vencida, asociación Product caída o autoridad revocada se normalizan como `409 enrollment_invalid`, igual que hoy.

## C9. Catálogo de auditoría

```text
installation_suspended
installation_reactivated
installation_rehost_authorized
installation_identity_retired
installation_binding_revoked
installation_rehost_completed
```

Campos: staff, installation, organization, product (si existe), timestamp, before/after, reason, result. Nunca private keys, tokens, device/user codes crudos, nonces ni contraseñas.

## C10. Eventos de seguridad

- Reutilizar `identity_revoked` al autorizar rehost (retiro de identidad/binding).
- Reutilizar `rehost_performed` al completar.
- `clone_suspected` permanece conceptual; C5 no promete detección fiable.
- Si se necesitan `rehost_authorized`/`rehost_expired`, se añadirán al CHECK de `commercial_security_event` en la migración futura.

## C11. Plan de migración (`0009`, implementada en rama)

Contenido previsto:

1. `installation.product_id UUID NULL` + FK a `product`.
2. FK compuesta `(organization_id, product_id) → organization_product`.
3. Trigger: `organization_id` inmutable; `product_id` inmutable una vez asignado.
4. Trigger: prohibir DELETE de `installation`.
5. `enrollment_request.product_id UUID NULL` + FK a `product`.
6. Tabla `installation_rehost_authorization` con `enrollment_request_id UNIQUE`, checks de expiración e índice único parcial `pending`.
7. Ampliación de `commercial_security_event` si se aprueban nuevos kinds.

No se modifica `0001–0008`. **C5.1 no hará `NOT NULL`**: `product_id` queda nullable para legacy y las nuevas instalaciones se exigen product-bound en application service. El backfill explícito, la verificación de cero NULL y el `SET NOT NULL` pertenecen a una fase posterior.

## C12. OpenAPI

- Enums cerrados publicados: `AdministrativeStatusDto` (`active|suspended|unregistered`), `IdentityStatusDto` (`unenrolled|active|retired`), `RehostStatusDto` (`pending|consumed|expired|cancelled`). El estado del binding se expone como `binding_active: bool` + `current_binding` (no se publica un enum adicional).
- Schemas Control propios; no reutilizar DTOs DEV.
- UUID con `format: uuid`; timestamps con `time::serde::rfc3339` explícito; `ErrorResponse`; cursor opaco.
- Security: `staffSession`.
- Sin exponer public key completa ni datos sensibles.

## C13. Frontend — arquitectura de información

```text
/control/installations
/control/installations/:installationId
```

Listado:

```text
Installation / device label
Organization
Product
Administrative status
Identity status
Last known activity
```

Detalle:

```text
Identity (UUID, generation, fingerprint)
Organization
Product (o Unassigned)
Administrative status / identity status
Registered / enrolled
Last known activity
Bindings (estado, fechas)
Rehost (estado, expiración, motivo)
Acciones: Suspend / Reactivate / Rehost según capability
```

- Acciones con confirmación tipada o diálogo explícito; rehost muestra advertencia de rotación de identidad.
- `409` se muestra como conflicto de estado, sin optimistic updates.
- Sin licencias, trial, billing, sync, online/offline ni métricas inventadas.
- Query keys: `["control","installations",filters]`, `["control","installation",id]`.

## C14. Matriz de pruebas requerida

- Migración `0009`: FKs, uniqueness parcial, expiración, estados terminales, no DELETE, inmutabilidad.
- Rehost: cutover atómico y rollback ante fallo; autorización ligada a `enrollment_request_id`; expiración conjunta con el request; expiración de pending vencida al crear otra; retry idempotente por `enrollment_request_id`; dos completions; clave vieja rechazada; snapshot org/product; rehost con Installation `suspended` conserva `suspended`.
- Product enrollment: begin/approve/proof con Product inexistente, fuera de allowlist (`development`, `deprecated`, `retired`), asociación inactive; resolución key→id; request hash/idempotencia; `maintenance` permitido sólo en rehost.
- RBAC: read sin manage; manage sin rehost; rehost sin manage; sin read; combinaciones independientes.
- HTTP: 200/201, 400, 401, 403, 404, 409 (transition/rehost/expired/product), cursor inválido, limit inválido, RFC3339, JSON de error.
- Storage: keyset, filtros, detalles sin N+1, historial de identidades/bindings.
- Frontend: list/detail current-only, filtros, paginación, 403, 401, 409, rehost, RBAC, fingerprint completo copiable, responsive 1920/1440/1024/768/390.
- Auditoría: mutación + evento atómicos; ausencia de secretos.

## C15. `devAdminToken` — coexistencia y deprecación

- Los cinco endpoints actuales permanecen LEGACY / DEV sin cambios.
- C5 no los consume desde Control Center.
- Documentar su carácter temporal y no productivo.
- Deprecación futura sólo cuando exista equivalente staff aprobado, con plan explícito y sin borrado silencioso.

## C16. Estado del gate C5.0C

```text
C5.0B: APROBADO
C5.0C: APROBADO
C5.1: IMPLEMENTADO EN RAMA / VALIDADO LOCALMENTE
C5.2: IMPLEMENTADO EN RAMA / VALIDADO LOCALMENTE
0009: IMPLEMENTADA EN RAMA / NO integrada en main
commit/push: rama publicada en PR #20
```
