# G4.2 — Plan de implementación por PR, pruebas y gates

- **Propósito:** dividir la implementación futura en PRs verificables y definir pruebas, gates y criterios de aceptación.
- **Estado:** PROPOSED — no iniciado.
- **Fecha:** 2026-11-08.
- **Alcance:** división futura del trabajo, pruebas obligatorias y criterios de aceptación.
- **Decisiones:** la secuencia depende de los gates G-1…G-5.
- **Dependencias:** ADR-0003 y aprobaciones D-A…D-J.
- **Riesgos:** §9 y §10.
- **Referencias:** §11.

## 1. Gates previos

Ningún PR de implementación comienza hasta:

| Gate | Criterio |
|---|---|
| G-1 | Titular aprueba ADR-0003 y decisiones D-A…D-J relevantes. |
| G-2 | Se confirma el rol que enrola y la autoridad del Trial. |
| G-3 | Se aprueba el modelo de datos y el plan de migraciones. |
| G-4 | Se aprueba el tratamiento del Trial local heredado. |
| G-5 | Se define custodia de claves y formato público. |

## 2. G4.2-B — Modelo persistente y enrolamiento

| Campo | Contenido |
|---|---|
| Alcance | Migraciones 0004+, identidad de instalación, `enrollment_request`, binding, idempotencia, auditoría, API de enrolamiento (inicio, aprobación, proof) y consulta de instalaciones. |
| Dependencias | G-1…G-5. |
| Módulos | `platform-core` (tipos de identidad y estados), `platform-application` (`EnrollmentService`, puertos), `platform-storage-postgres` (repositorio y SQL), `apps/api` (DTOs, handlers, OpenAPI), `openapi/`. |
| Migraciones | `0004_installation_identity.sql`, `0005_enrollment.sql`, `0006_audit_security_events.sql` (nombres tentativos). |
| Contratos | `POST /enrollment-requests`, `.../approve`, `.../proof`, `GET /organizations/{id}/installations` (y detalle si se aprueba). |
| Pruebas | Ver sección 4, grupos 1–4 y 5 parcial. |
| Riesgos | Autorización incorrecta, replay, conflictos de identidad, crecimiento de solicitudes. |
| Aceptación | Owner enrola; admin/member no; tenant ajeno 404; replay 409/410; duplicados 409; auditoría transaccional; OpenAPI 3.1; snapshot. |

## 3. G4.2-C — Elegibilidad y activación Trial

| Campo | Contenido |
|---|---|
| Alcance | `trial_entitlement`, política por organización, activación idempotente, denegación auditada, prevención de reinicio. |
| Dependencias | G4.2-B; decisión D-C/D-D. |
| Módulos | `platform-application` (política), PostgreSQL, API, OpenAPI, docs. |
| Migraciones | `0007_trial_entitlement.sql` y ajustes de auditoría. |
| Contratos | `POST /organizations/{id}/installations/{installation_id}/trial`. |
| Pruebas | Sección 4, grupos 3–6. |
| Riesgos | Concurrencia de activación, Trials heredados, abuso multi-cuenta. |
| Aceptación | Una activación por organización; repetir es idempotente; reinstalar no reinicia; eventos auditados. |

## 4. G4.2-D — Integración cliente POS

| Campo | Contenido |
|---|---|
| Alcance | Generación/custodia de clave, protocolo HTTPS, UI de vinculación, recepción de grants, verificación Rust, reconciliación con el Trial local y recuperación. |
| Dependencias | G4.2-C aprobado y desplegado en un entorno de pruebas. |
| Módulos POS | `pos-licensing`, `pos-application`, `pos-storage-sqlite`, `apps/pos-desktop`, migración local incremental. |
| Contratos | Consumo de los endpoints de G4.2-B/C; sin cambios de `LicenseGrant` v1 salvo decisión v2. |
| Pruebas | Sección 4, grupos 5–7. |
| Riesgos | Custodia de clave por plataforma, migración del Trial local, compatibilidad Windows/Linux/Android. |
| Aceptación | Enrolamiento real desde POS; operación offline tras grant; reinstalación y restore no extienden Trial; clave privada nunca sale. |

## 5. G4.2-E — Endurecimiento y E2E

| Campo | Contenido |
|---|---|
| Alcance | Rehost, clonación, rotación, reconexión, errores, rate limits, compatibilidad de versiones y pruebas físicas. |
| Dependencias | G4.2-D. |
| Módulos | Ambos repositorios. |
| Migraciones | Solo si el endurecimiento exige nuevas columnas aprobadas. |
| Contratos | Versión compatible; sin romper clientes anteriores. |
| Pruebas | Sección 4, grupos 4–7 completos. |
| Riesgos | Falsos positivos de clonación, soporte operativo, deriva de versiones. |
| Aceptación | Matriz de amenazas verificada; rehost y clonación demostrados; E2E real estable. |

## 6. Plan de pruebas obligatorio

### 6.1 Unitarias (Rust)

- Canonicalización de la prueba (`hiqnet-enrollment-proof:v1`).
- Validación de UUID, clave, etiqueta y versión.
- Máquina de estados de `enrollment_request`.
- Reglas de elegibilidad (`available`/`consumed`/`revoked`).
- Decisión de rehost y motivos.

### 6.2 Integración PostgreSQL

- Enrolamiento válido con owner y organización activa.
- Actor sin permiso (admin/member) → 403.
- Organización ajena → 404.
- Cuenta deshabilitada → 401/404 según capa.
- Challenge expirado → 410.
- Challenge reutilizado → 409.
- Firma incorrecta → 400/401.
- Clave sustituida → 409.
- Registro duplicado (UUID/clave/etiqueta) → 409.
- Dos solicitudes concurrentes → una sola vinculación.
- Timeout tras commit + reintento idempotente.
- Rollback si falla la auditoría.
- Activación de Trial concurrente → una sola.
- Reinstalación en la misma organización → sin Trial nuevo.
- Rehost autorizado → binding anterior revocado y evento.

### 6.3 Contrato OpenAPI

- Las rutas candidatas aparecen con seguridad correcta.
- Requests/responses y errores documentados.
- Snapshot estable.
- Rutas DEV no aceptan cookie comercial y viceversa.

### 6.4 Seguridad

- Replay de proof.
- Cross-tenant en aprobación, proof y listados.
- CSRF en aprobación.
- Rate limit en endpoints públicos.
- Ausencia de claves privadas en cuerpos, logs y auditoría.
- Manipulación de grant en el POS.
- Confusión DEV/comercial.
- Enumeración por UUID.

### 6.5 E2E real POS ↔ Platform

- Instalación nueva → enrolamiento → activación Trial → operación offline.
- Reconexión y reconciliación.
- Cambio de organización intentado por el cliente → rechazo.
- Rehost.
- Clonación (dos perfiles con la misma clave).
- Restauración de backup.
- Versión POS antigua contra Platform nueva y viceversa (compatibilidad).

### 6.6 Offline

- Operación durante la vigencia del grant sin Internet.
- Vencimiento → solo lectura.
- Intento de renovación local → rechazo.
- Reloj atrasado/adelantado.
- Restauración de backup antiguo.

### 6.7 Reinstalación y recuperación

- Formateo conservando clave.
- Formateo sin clave → rehost.
- Pérdida de clave → rehost.
- Restore en el mismo equipo.
- Restore en equipo distinto.

## 7. Automatización

| Propiedad | Automatizable |
|---|---|
| Aislamiento y RBAC | Sí (HTTP + PostgreSQL). |
| Consumo único, replay, idempotencia | Sí. |
| Conflictos de clave/tenant | Sí. |
| Rollback de auditoría | Sí. |
| Trial único por organización | Sí. |
| Contratos OpenAPI | Sí. |
| Verificación Ed25519 en POS | Sí. |
| Clonación con copia física de perfil | Parcial; requiere E2E/físico. |
| Manipulación del reloj offline | Parcial; requiere prueba física y aceptación. |
| Restore en hardware distinto | Parcial; E2E/físico. |
| Compatibilidad Android real | Requiere dispositivo/emulador validado. |

## 8. Criterios de aceptación globales

- El enrolamiento exige sesión, organización autorizada y prueba de clave.
- El tenant no puede fijarse desde el cliente.
- Replay, duplicados y carreras están cubiertos.
- El Trial no se reinicia por reinstalación en la misma organización.
- La auditoría es transaccional e inmutable.
- La clave privada nunca sale del POS.
- El grant sigue verificándose offline.
- OpenAPI 3.1, snapshot y CI en verde.
- Sin migraciones editadas ni secretos.

## 9. Estimación de riesgo por PR

| PR | Riesgo principal | Mitigación |
|---|---|---|
| G4.2-B | Autorización y replay | Pruebas cross-tenant y concurrencia. |
| G4.2-C | Doble Trial concurrente | Transición condicional única y pruebas. |
| G4.2-D | Custodia de clave y migración local | Matriz de plataformas y recuperación. |
| G4.2-E | Falsos positivos y compatibilidad | Ventanas de prueba y telemetría mínima. |

## 10. Dependencias y riesgos generales

- **Depende de:** gates G-1…G-5 y de la aprobación del ADR-0003.
- **Riesgo de secuencia:** iniciar G4.2-C antes de estabilizar G4.2-B reintroduce carreras de Trial.
- **Riesgo de compatibilidad:** un cambio de grant en G4.2-D puede romper POS antiguos; exige ventana de verificación dual.
- **Riesgo operativo:** el soporte de rehost y clonación requiere runbook y criterios aprobados.
- **Mitigación:** PRs pequeños, migraciones append-only y CI con PostgreSQL real.

## 11. Referencias

- [ADR-0003](adr-0003-installation-enrollment.md)
- [Modelo de amenazas](g42-enrollment-threat-model.md)
- [Protocolo](g42-enrollment-protocol.md)
- [Modelo de datos](g42-data-model.md)
- [Política Trial](g42-trial-eligibility-rehost-policy.md)
- [Matriz de decisiones](g42-decisions-matrix.md)
