# G4.2 — Matriz de decisiones, contratos candidatos e idempotencia

- **Propósito:** comparar alternativas, fijar recomendaciones provisionales y documentar contratos candidatos e idempotencia.
- **Estado:** PROPOSED — ninguna decisión de este documento está aprobada.
- **Fecha:** 2026-11-08.
- **Alcance:** decisiones A–D, matriz HTTP candidata y escenarios de duplicados.
- **Decisiones:** §1–§4 y §6.
- **Dependencias:** ADR-0003, protocolo, datos y política Trial.
- **Riesgos:** §7.
- **Referencias:** §8.

## 1. Decisión A — Autoridad para enrolar

| Criterio | A: `owner` | B: `owner`+`admin` | C: permiso específico |
|---|---|---|---|
| Menor privilegio | Alto | Medio | Alto |
| Delegación operativa | Baja | Alta | Alta |
| Complejidad de implementación | Baja | Baja | Alta |
| Impacto de cuenta comprometida | Bajo | Alto | Medio |
| Coherencia con RBAC actual | Total | Total | Requiere extensión |
| Riesgo de error de autorización | Bajo | Medio | Medio |
| **Recomendación** | **Sí (G4.2-B)** | Reevaluar | No ahora |

**Motivo:** el mercado objetivo son negocios pequeños; el `owner` es el responsable natural de los equipos. La delegación puede añadirse después sin cambiar el modelo de datos (un futuro permiso `installation.manage`).

**Estado:** PROPOSED, pendiente de aprobación.

## 2. Decisión B — Identidad de instalación

| Criterio | UUID | Token 1 uso | Clave local | Challenge | Token+clave | TPM | Recuperación |
|---|---|---|---|---|---|---|---|
| Prueba de posesión | No | Parcial | Sí | Sí | Sí | Sí | Parcial |
| Resistencia a replay | No | Media | Alta | Alta | Alta | Alta | Media |
| Portabilidad | Total | Total | Alta | Alta | Alta | Baja | Total |
| Reinstalación | Trivial | Token nuevo | Clave conservada | Igual | Igual | Compleja | Rehost |
| Coste | Nulo | Bajo | Bajo | Bajo | Bajo | Alto | Bajo |
| Offline | Sí | No aplica | Sí | Sí | Sí | Sí | Sí |
| **Veredicto** | Rechazada | Complementaria | Base | Base | **Recomendada** | Opcional | Necesaria |

**Recomendación:** clave Ed25519 local + challenge de servidor + token/código de aprobación + rehost auditado. La clave privada se protege con el keystore del SO cuando exista; archivo restringido como fallback. TPM opcional.

**Estado:** PROPOSED.

## 3. Decisión C — Idempotencia y duplicados

| Escenario | Resultado propuesto | Mecanismo | Evento |
|---|---|---|---|
| Mismo `installation_id`, misma organización, mismo payload | Devolver resultado previo | `Idempotency-Key` + hash | Ninguno |
| Mismo `installation_id`, organización diferente | `409` + evento | Binding activo inmutable | `tenant_change_attempt` |
| Misma clave pública, `installation_id` distinto | `409` + evento | Índice único de fingerprint activo | `duplicate_key` |
| Misma etiqueta, instalación distinta | `409` | `UNIQUE(organization_id, device_label)` existente | Ninguno |
| Dos solicitudes simultáneas de prueba | Una `201`, otra `409` | Consumo condicional/`FOR UPDATE` | `replay_rejected` si aplica |
| Timeout tras commit | Reintento devuelve resultado | Registro de idempotencia | Ninguno |
| Reintento misma clave, payload distinto | `409` | Hash distinto | `enrollment_conflict` |
| Clonación de instalación | `409` + revisión | Fingerprint duplicado | `duplicate_key` |
| Restauración de backup antiguo | Sin extensión de Trial | Estado comercial preservado | Ninguno |
| Rehost explícito | `201` nuevo binding + revocación | Transacción | `rehost_performed` |

**Reglas:**

- La idempotencia es persistente, nunca solo en memoria.
- El hash cubre método, ruta lógica y cuerpo canónico.
- El consumo de challenge es único por transición condicional.
- Un conflicto de identidad no se resuelve automáticamente: requiere rehost o soporte.

**Estado:** PROPOSED.

## 4. Decisión D — Autoridad del Trial

| Opción | Reinstalación | Multi-negocio legítimo | Abuso multi-cuenta | Complejidad |
|---|---|---|---|---|
| Por instalación | Reinicia | Sin impacto | Alto | Baja |
| Por cuenta | No reinicia | Puede bloquear | Medio | Media |
| Por organización | No reinicia | Correcto | Alto con orgs nuevas | Media |
| Organización + señales de cuenta | No reinicia | Correcto con revisión | Medio | Alta |
| Historial central + verificación | No reinicia | Correcto | Bajo-medio | Alta |

**Recomendación:** organización como unidad, con `first_seen_account_id` y señales; “un Trial por cuenta” queda PENDIENTE.

**Estado:** PROPOSED.

## 5. Contratos HTTP candidatos

Ninguna ruta está aprobada ni implementada. Todas usan HTTPS y respuestas de error normalizadas.

### 5.1 `POST /api/v1/enrollment-requests`

| Campo | Valor |
|---|---|
| Actor | Ninguno (público) |
| Autorización | Rate limit por IP/installation; sin sesión |
| Request | `installation_id`, `public_key`, `device_label`, `app_version`, `platform` |
| Response | `201` con `device_code`, `user_code`, `verification_uri`, `nonce`, `expires_in` |
| Errores | 400, 409, 429, 503 |
| Estado previo | Sin solicitud |
| Estado posterior | `pending` |
| Idempotencia | `Idempotency-Key` opcional |
| CSRF | No aplica (sin cookies) |
| Rate limit | Sí |
| Prueba de posesión | No en este paso; se exige en proof |
| Auditoría | Evento técnico de creación; sin datos sensibles |
| Tenant | Ninguno todavía |

### 5.2 `POST /api/v1/enrollment-requests/{user_code}/approve`

| Campo | Valor |
|---|---|
| Actor | Cuenta comercial autenticada |
| Autorización | `owner` activo de la organización elegida |
| Request | `organization_id` |
| Response | `200`/`204` aprobado |
| Errores | 400, 401, 403, 404 neutral, 409, 429, 503 |
| Estado previo | `pending` |
| Estado posterior | `approved` con `organization_id` y actor |
| Idempotencia | Repetir devuelve estado actual |
| CSRF | `Origin` exacto |
| Rate limit | Sí |
| Prueba de posesión | No; la aporta el POS en proof |
| Auditoría | `enrollment_approved` (propuesto) |
| Tenant | Fijado aquí; nunca por el POS |

### 5.3 `POST /api/v1/enrollment-requests/{device_code}/proof`

| Campo | Valor |
|---|---|
| Actor | Dispositivo (prueba criptográfica) |
| Autorización | Solicitud aprobada + firma válida |
| Request | `installation_id`, `public_key`, `signature` |
| Response | `201` con instalación y binding |
| Errores | 400, 404 neutral, 401, 409, 410, 429, 503 |
| Estado previo | `approved` |
| Estado posterior | `completed` + instalación vinculada |
| Idempotencia | Consumo único; reintento con misma clave devuelve resultado |
| CSRF | No aplica (sin cookies) |
| Rate limit | Sí |
| Prueba de posesión | Firma Ed25519 sobre nonce canónico |
| Auditoría | `installation_enrolled` en la misma transacción |
| Tenant | El de la aprobación |

### 5.4 `GET /api/v1/organizations/{id}/installations`

| Campo | Valor |
|---|---|
| Actor | Cualquier miembro activo |
| Autorización | Membresía activa; 404 si es ajena |
| Request | `limit`, `cursor` |
| Response | `200` con `items` y `next_cursor` |
| Errores | 400, 401, 404, 503 |
| Estado | Sin mutación |
| Idempotencia | GET seguro |
| CSRF | No aplica |
| Rate limit | Técnico |
| Prueba de posesión | No aplica |
| Auditoría | No |
| Tenant | Filtrado en SQL |

Datos expuestos mínimos: `installation_id`, `device_label`, `status`, `identity_status`, `bound_at`, `last_seen_at`. Sin claves públicas completas ni datos personales innecesarios.

### 5.5 `GET /api/v1/organizations/{id}/installations/{installation_id}`

Igual que 5.4, con `404` neutral cross-tenant. Puede posponerse si el listado es suficiente.

### 5.6 Activación de Trial (fase G4.2-C)

| Campo | Valor |
|---|---|
| Ruta candidata | `POST /api/v1/organizations/{id}/installations/{installation_id}/trial` |
| Actor | `owner` (o rol aprobado) |
| Autorización | Membresía + organización activa + instalación vinculada |
| Request | Vacío o `Idempotency-Key` |
| Response | `200`/`201` con `status`, `started_at`, `ends_at` |
| Errores | 401, 403, 404, 409 (ya consumido), 429, 503 |
| Estado previo | `available` |
| Estado posterior | `consumed` |
| Idempotencia | Obligatoria |
| CSRF | `Origin` exacto |
| Auditoría | `trial_activated`/`trial_denied` |
| Tenant | El de la ruta, validado por membresía |

### 5.7 Matriz DEV vs comercial

| Ruta | Credencial | Propósito | ¿Comercial? |
|---|---|---|---|
| `POST /api/v1/installations` | `devAdminToken` | Soporte/desarrollo | No |
| `GET /api/v1/installations/{id}` | `devAdminToken` | Soporte/desarrollo | No |
| `POST /api/v1/installations/{id}/trial` | `devAdminToken` | Soporte/desarrollo | No |
| `POST /api/v1/installations/{id}/licenses` | `devAdminToken` | Emisión DEV | No |
| `POST /api/v1/enrollment-requests` | Público + rate limit | Enrolamiento | Sí |
| `.../approve` | `commercialSession` | Aprobación | Sí |
| `.../proof` | Firma de dispositivo | Prueba | Sí |
| `GET /api/v1/organizations/{id}/installations` | `commercialSession` | Consulta | Sí |
| `POST .../trial` | `commercialSession` | Trial | Sí |

Regla: una ruta DEV no puede aceptar cookie comercial para actuar como cuenta, y una ruta comercial no puede aceptar el token DEV.

## 6. Decisiones pendientes

| ID | Decisión | Alternativas | Recomendación | Riesgo | Estado |
|---|---|---|---|---|---|
| D-A | Rol que enrola | owner / owner+admin / permiso | owner | Delegación limitada | PENDIENTE |
| D-B | Identidad de dispositivo | UUID / token / clave / clave+token / TPM | clave+token con fallback | Clonación sin hardware | PENDIENTE |
| D-C | Autoridad Trial | instalación / cuenta / organización / combinación | organización + señales | Abuso multi-cuenta | PENDIENTE |
| D-D | Trial local heredado | mantener / exigir online / provisional | provisional + legacy | Reinicio hasta G4.2-D | PENDIENTE |
| D-E | Rehost | automático / revisión | automático auditado; umbral a decidir | Abuso de owner | PENDIENTE |
| D-F | Formato `key_id` | huella completa / truncada | truncada con versión | Colisión improbable | PENDIENTE |
| D-G | Expiración de challenge | 5 / 10 / 15 min | 10 min | Ventana de replay | PENDIENTE |
| D-H | Verificación de correo | sí / no / fase posterior | sí antes de abuso masivo | Trials múltiples | PENDIENTE |
| D-I | Custodia de clave | keystore / archivo | keystore con fallback | Robo local | PENDIENTE |
| D-J | Retención de eventos | 90d / 1a / indefinido | 1 año para seguridad | Crecimiento | PENDIENTE |

## 7. Dependencias y riesgos

- **Depende de:** ADR-0003 y de las aprobaciones D-A…D-J.
- **Riesgo principal:** tomar una decisión operativa (por ejemplo, rehost automático) sin umbral aprobado.
- **Riesgo de contrato:** publicar rutas candidatas como si fueran definitivas antes de la revisión.
- **Mitigación:** toda decisión permanece PENDIENTE hasta aprobación expresa del titular.

## 8. Referencias

- [ADR-0003](adr-0003-installation-enrollment.md)
- [Protocolo](g42-enrollment-protocol.md)
- [Modelo de datos](g42-data-model.md)
- [Política Trial](g42-trial-eligibility-rehost-policy.md)
- [Plan de implementación y pruebas](g42-implementation-plan.md)
