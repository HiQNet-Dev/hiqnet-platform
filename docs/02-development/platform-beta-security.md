# HiQNet Platform — Seguridad de Beta 0.1.0

**Estado:** RATIFICADO (Fase 1, 2026-11-09) · VALIDADO (B9, 2026-11-09)
**Alcance:** revisión de seguridad para la Beta administrativa web
**Relacionado:** [Arquitectura rectora](../01-architecture/platform.md) · [ADR-0006](../01-architecture/adr-0006-staff-rbac.md) · [Matriz RBAC](control-center-rbac-matrix.md) · [Backlog Beta](platform-beta-backlog.md)

> Este documento distingue riesgos bloqueantes de Beta, riesgos aceptados temporalmente y riesgos diferidos. No cambia la matriz RBAC ni implementa controles.

## 1. Controles existentes

| Área | Control real | Estado |
|---|---|---|
| Password staff/comercial | Argon2id, política de contraseñas | Implementado |
| Sesiones | Persistidas en PostgreSQL, token hash, expiración, revocación | Implementado |
| Cookies staff | `hiqnet_staff_session`, `HttpOnly`, `SameSite=Strict`, `Path=/api/v1/control`, `Secure` por defecto | Implementado |
| Cookies comercial | `hiqnet_session`, `HttpOnly`, `SameSite=Lax`, `Path=/api`, `Secure` por defecto | Implementado |
| CSRF | Validación de `Origin` exacto en producción | Implementado |
| Rate limiting | Login, organizaciones, enrolamiento; saturación Argon2id | Implementado |
| Lockout | 5 fallos → 15 minutos | Implementado |
| RBAC | 6 roles, 17 capabilities, deny-by-default, 403/404 neutrales | Implementado |
| Aislamiento tenant | SQL con `organization_id`, constraints, tests cross-tenant | Implementado |
| Auditoría | `staff_audit_event`, `commercial_security_event`, `staff_auth_event`, append-only UPDATE/DELETE | Implementado |
| Secretos | Sin claves privadas en respuestas; metadata saneada en C6 | Parcial / contrato |
| Errores | `ErrorResponse` normalizado, sin SQL ni trazas internas | Implementado |
| DEV | `devAdminToken`, `ALLOW_DEV_ISSUANCE`, bootstrap DEV | Debe deshabilitarse en producción |
| HTTPS | Esperado en despliegue; cookie `Secure` | Configuración de despliegue |

### 1.1 Perfiles de exposición

| Perfil | Alcance | Decisión |
|---|---|---|
| **Beta interna restringida** | Sólo personal staff; sin clientes externos | Permitida con las condiciones comunes de §5.1 |
| **Beta externa** | Customer Portal accesible a clientes invitados con datos reales | Permitida sólo con §5.1 + §5.2 + aceptación explícita de MFA pendiente |
| **Producción pública** | Autoservicio o audiencia amplia | **No es Beta**; exige MFA/step-up, reducción de read caps, retención, escaneo de dependencias y hardening de `TRUNCATE` |

Un riesgo aceptado para la Beta interna **no** se acepta automáticamente para la Beta externa ni para producción.

### 1.2 Requisitos obligatorios por perfil

| Control | Interna | Externa | Pública |
|---|---|---|---|
| HTTPS/TLS | Obligatorio | Obligatorio | Obligatorio |
| HSTS, CSP, XFO, XCTO, Referrer-Policy | Obligatorio | Obligatorio | Obligatorio |
| Cookies `Secure`, `HttpOnly`, `SameSite` y `Origin` exacto | Obligatorio | Obligatorio | Obligatorio |
| Revocación de sesión y TTL acotado | Obligatorio | Obligatorio (TTL más corto) | Obligatorio |
| Aislamiento tenant y 404 neutral | Obligatorio | Obligatorio | Obligatorio |
| Configuración productiva sin DEV | Obligatorio | Obligatorio | Obligatorio |
| Auditoría y metadata saneada | Obligatorio | Obligatorio | Obligatorio |
| Migraciones y backup/restore validados | Obligatorio | Obligatorio | Obligatorio |
| MFA/step-up | Pendiente aceptado | Pendiente con compensación + aceptación escrita | **Obligatorio** |
| Reducción de `security.read`/`audit.read` | Recomendada | Recomendada | **Obligatoria** |
| Retención, `TRUNCATE`, escaneo de dependencias | Diferido | Diferido con aceptación | **Obligatorio** |

## 2. Revisión de `security.read` y `audit.read`

Hecho actual:

| Capability | super_admin | platform_admin | operator | support | security_auditor | read_only |
|---|---|---|---|---|---|---|
| `security.read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `audit.read` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| `security.manage` | ✓ | — | — | — | ✓ | — |
| `installations.rehost` | ✓ | ✓ | — | — | — | — |

Análisis:

- `security.read` y `audit.read` están concedidas a todos los roles, incluidos `support` y `read_only`.
- Los read models C6 exponen identificadores, nombres de organización/instalación/cuenta, actor staff y metadata saneada. No deben exponer tokens, hashes de sesión, IP/UA, claves ni JSONB crudo.
- El riesgo es de **confidencialidad interna**, no de escalada de privilegios: leer auditoría no concede mutaciones ni `installations.rehost`.
- `security_auditor` no tiene `installations.rehost`; correcto.
- `security.manage` queda reservada y no tiene superficie C6.
- Para una Beta interna con personal de confianza y datos saneados, la matriz es **aceptable**. Antes de ampliar a contratistas, clientes o un equipo grande, se recomienda restringir `security.read` a `super_admin`, `platform_admin`, `operator`, `security_auditor` mediante ADR explícito. No se cambia ahora.

**Decisión:** mantener la matriz ratificada; aceptar el riesgo con las condiciones de §5.

**Conclusión:** la matriz puede mantenerse para una **Beta restringida** con datos reales siempre que se cumplan las condiciones de §5 (metadata saneada, superficie staff interna, configuración productiva segura). No requiere ajustes mínimos antes de la Beta restringida; sí requiere un ADR de reducción antes de ampliar la audiencia.

### 2.1 Cuenta staff comprometida

Una cuenta staff comprometida puede leer los datos permitidos por su rol, ejecutar las mutaciones permitidas y generar auditoría; no puede evadir autenticación, auditoría ni controles de sesión. El mayor blast radius es `super_admin`, seguido de `platform_admin` (incluye `installations.rehost` y, en Beta, gestión comercial). Mitigaciones actuales: sesión corta, revocación, lockout, rate limit, Origin, cookies HttpOnly/Strict y auditoría. MFA/step-up se difiere con aceptación explícita; antes de producción pública debe reevaluarse.

### 2.2 Metadatos de auditoría

- C6 debe aplicar whitelist por acción/kind y devolver `{}` para eventos desconocidos.
- Nunca se envía JSONB crudo al navegador.
- No se exponen tokens, hashes de sesión, IP/UA, claves, nonces, device/user codes ni contraseñas.
- `reason` es texto libre y sólo se muestra en detalle; no se usa para conceder derechos.
- La auditoría y los eventos de seguridad son append-only para UPDATE/DELETE; `TRUNCATE` queda como deuda de privilegios DB.

### 2.3 Aceptación explícita de riesgos

- **S2 (media):** aceptado para staff interno con metadata saneada; no aceptado para contratistas o audiencia amplia; el Customer Portal no accede a auditoría/seguridad.
- **S1 (alta):** aceptado para Beta interna con las mitigaciones existentes; para Beta externa sólo con §5.2 y aceptación escrita; **no aceptado** para producción pública sin MFA/step-up o mitigación equivalente.
- **S3/S4/S5/S12/S16/S19/S20:** bloqueantes; no se aceptan sin mitigación verificable.
- **S6/S7/S8/S9:** diferidos con justificación y revisión en B9.

## 3. Matriz de riesgos

| ID | Riesgo | Impacto | Probabilidad | ¿Bloquea Beta? | Mitigación / decisión |
|---|---|---|---|---|---|
| S1 | Robo de sesión staff sin MFA | Alto | Media | No para Beta interna; externa sólo con compensación y aceptación; **sí** para producción pública | Sesión corta, revocación, HttpOnly/Strict, Origin, lockout, rate limit; externa: invitación, monitorización, TTL corto y aceptación escrita; pública: MFA/step-up |
| S2 | `support`/`read_only` leen auditoría/seguridad | Medio | Media | No con datos saneados y staff interno | Mantener matriz; reducir antes de contratistas/audiencia amplia; el Customer Portal no accede a auditoría |
| S3 | Endpoints DEV activos en producción | Alto | Baja con disciplina | **Sí** | Checklist de producción: token vacío, `ALLOW_DEV_ISSUANCE=false`, docs off, bootstrap off |
| S4 | Custodia de clave de licencia / firma DEV | Alto | Media | No si no hay emisión productiva | No emitir en producción; ADR de custodia antes de POS |
| S5 | Sin CSP/headers de seguridad en la app | Medio | Media | **Sí** para cualquier Beta desplegada | Configurar CSP, HSTS, XFO, XCTO, Referrer-Policy en proxy/static host (§5.1) |
| S6 | `TRUNCATE` no bloqueado por triggers | Medio | Baja | No | Hardening de privilegios DB; backups; post-Beta |
| S7 | Sin retención de auditoría | Bajo/Medio | Alta a largo plazo | No | Política de retención/particionado post-Beta |
| S8 | Sin escaneo de dependencias/secretos completo | Medio | Media | No | Ejecutado en B9 (§7.5); ver riesgos residuales aceptados |
| S9 | `staff_auth_event` incompleto | Bajo | Alta | No | C6 documenta; añadir writers `session_revoked`/`account_locked`/`password_changed` |
| S10 | `super_admin` con blast radius amplio | Alto | Baja | No | Auditoría, least privilege, step-up futuro; sin bypass |
| S11 | XSS en frontends | Medio | Media | No | React escaping, CSP, no HTML crudo; revisar en B9 |
| S12 | Fuga cross-tenant | Alto | Baja | **Sí si falla** | Tests de aislamiento, SQL con tenant, 404 neutral; mantener regresión |
| S13 | Secretos en logs/tracing | Alto | Baja | No | Sanitización, no loggear cuerpos/URIs sensibles; revisar |
| S14 | CSRF comercial | Medio | Baja | No | Origin exacto, SameSite=Lax, cookies HttpOnly |
| S15 | Fuerza bruta/enumeración | Medio | Media | No | Rate limit, lockout, mensajes neutrales |
| S16 | Configuración productiva incorrecta | Alto | Media | **Sí** | Runbook y checklist de despliegue |
| S17 | Emisión de licencias sin elegibilidad | Alto | Baja | No si está deshabilitada | Mantener `ALLOW_DEV_ISSUANCE=false`; elegibilidad en fase POS |
| S18 | Dependencias vulnerables | Medio | Media | No | Escaneo y actualización en B9 (§7.5) |
| S19 | Backups no probados | Alto | Media | **Sí** | Validar backup/restore en B9 |
| S20 | Migraciones no reproducibles | Alto | Baja | **Sí** | Migración limpia + incremental en B9 |

## 4. Bloqueantes vs diferibles

### Bloqueantes de Beta

1. Configuración productiva segura (S3, S16).
2. TLS, cookies seguras, Origin exacto y headers HSTS/CSP/XFO/XCTO/Referrer-Policy (S5).
3. Aislamiento tenant verificado (S12).
4. Migraciones reproducibles y backup/restore validado (S19, S20).
5. Sin emisión productiva de licencias sin custodia aprobada (S4, S17).
6. Datos de auditoría/seguridad saneados antes de exponer C6.

### Diferibles con justificación

- MFA/step-up (S1): Beta interna aceptada; Beta externa requiere compensación de §5.2 y aceptación escrita; producción pública requiere MFA.
- `TRUNCATE`/privilegios DB (S6): riesgo bajo con backups y acceso restringido.
- Retención (S7): no afecta funcionalidad; riesgo de crecimiento.
- Escaneo de dependencias (S8, S18): añadir en B9 o aceptar formalmente.
- Staff auth events faltantes (S9): C6 los documenta; no bloquean la Beta.
- Restricción de `security.read`/`audit.read` (S2): aceptar para staff interno; reducir antes de contratistas/audiencia amplia.
- 4-eyes, incident management, SIEM: fuera de alcance.

## 5. Condiciones de seguridad para declarar Beta

### 5.1 Comunes (interna y externa)

- [ ] `HIQNET_PLATFORM_ENV=production`.
- [ ] `HIQNET_PLATFORM_COOKIE_SECURE=true`.
- [ ] `HIQNET_PLATFORM_ALLOWED_ORIGIN` con orígenes exactos (lista separada por comas, sin comodines).
- [ ] `HIQNET_PLATFORM_ADMIN_TOKEN` vacío.
- [ ] `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE=false`.
- [ ] `HIQNET_PLATFORM_ENABLE_DOCS=false` salvo decisión explícita.
- [ ] `HIQNET_PLATFORM_DEV_BOOTSTRAP` y `HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP` deshabilitados.
- [ ] TLS terminado en proxy; HSTS, CSP, XFO, XCTO y Referrer-Policy configurados.
- [ ] Cookies `Secure`, `HttpOnly`, `SameSite` y `Path` correctos en staff y comercial.
- [ ] Revocación de sesión y TTL acotado verificados.
- [ ] Aislamiento tenant y 404 neutral verificados.
- [ ] Migración desde base limpia e incremental verificada.
- [ ] Backup/restore ensayado en base desechable.
- [ ] Auditoría/seguridad sin secretos y con metadata saneada.
- [ ] Sin emisión productiva de licencias hasta aprobar custodia de clave.
- [ ] Riesgos S5/S6/S7/S8/S9 aceptados por escrito o mitigados.

### 5.2 Externas (Customer Portal con clientes reales)

- [ ] Acceso por invitación; tenants limitados; sin autoservicio de alta.
- [ ] Monitorización de fallos de login y eventos de seguridad (C6).
- [ ] TTL de sesión más corto que en la Beta interna.
- [ ] Contacto de incidente y procedimiento de revocación de sesiones.
- [ ] S1 aceptado por escrito con compensación (invitación, monitorización, TTL); MFA/step-up planificado.
- [ ] S2: staff interno; reducción de `security.read`/`audit.read` recomendada antes de contratistas o audiencia amplia.
- [ ] El Customer Portal no accede a `/control`, auditoría/seguridad ni datos de otras organizaciones.
- [ ] Productos, plan, suscripción y entitlements se exponen en read-only y sin datos de pago.
- [ ] Sin autoservicio de pagos, cambios de plan o cancelaciones.

### 5.3 Pendientes documentados

- MFA/step-up.
- Reducción de `security.read`/`audit.read` para audiencias amplias.
- Retención/particionado de auditoría.
- Hardening de `TRUNCATE`/privilegios DB.
- Escaneo de dependencias/secretos.
- Writers faltantes de `staff_auth_event`.

## 6. Relación con POS

- La Beta de Platform no emite licencias productivas ni exige Tauri/Android.
- La integración POS (Fase 8) requerirá:
  - custodia de clave de firma;
  - elegibilidad basada en suscripción;
  - auditoría de emisión;
  - compatibilidad `LicenseGrant` v1;
  - operación offline y renovación;
  - prueba de posesión Ed25519.
- No se introduce ninguna dependencia runtime de Platform para la venta local del POS.

## 7. Resultados de validación B9 (2026-11-09)

Esta sección no cambia las decisiones ratificadas. Registra la evidencia obtenida en B9 y los riesgos residuales que se aceptan formalmente para la Beta.

### 7.1 Controles verificados en código

| Control | Verificación | Resultado |
|---|---|---|
| Cookie staff | `STAFF_SESSION_COOKIE` con `HttpOnly; Path=/api/v1/control; SameSite=Strict` y `Secure` salvo DEV explícito | Conforme |
| Cookie comercial | `SESSION_COOKIE` con `HttpOnly; Path=/api; SameSite=Lax` y `Secure` salvo DEV explícito | Conforme |
| `Secure` en producción | `HIQNET_PLATFORM_COOKIE_SECURE=false` aborta el arranque si `HIQNET_PLATFORM_ENV!=development` | Conforme |
| Origin | `origin_allowed` exige coincidencia exacta y rechaza múltiples cabeceras; sólo DEV permite ausencia | Conforme |
| Mutaciones staff | `require_origin` precede toda mutación de Control Center (productos, proyectos, organización-producto, instalaciones, planes, suscripciones) | Conforme |
| RBAC | 17 capabilities en `platform-core`; `security.manage` reservada sin superficie | Conforme |
| DEV | `dev-bootstrap-account`/`dev-bootstrap-staff` exigen `ENV=development` + flag; token admin opcional vacío deshabilita mutaciones DEV; `/docs` tras `HIQNET_PLATFORM_ENABLE_DOCS` | Conforme |
| SQL | Sin SQL construido con entrada de usuario; los `format!` sólo concatenan constantes `SELECT` con `WHERE` literal y bindings | Conforme |
| Aislamiento tenant | Pruebas de organización-producto, instalaciones, dominio comercial y E2E de Portal cubren cross-tenant y 404 neutral | Conforme |

### 7.2 Matriz RBAC

La matriz de [control-center-rbac-matrix.md](control-center-rbac-matrix.md) coincide exactamente con `StaffRole::capabilities` en `crates/platform-core/src/staff.rs`. `security.read` y `audit.read` siguen concedidas a todos los roles según §2; `security.manage` no tiene endpoint. **No se requieren ajustes para la Beta restringida.**

### 7.3 Riesgo de cuenta staff comprometida

Sin MFA, el blast radius máximo es `super_admin` y el siguiente `platform_admin`. Las mitigaciones verificadas (sesión con TTL, revocación, lockout 5/15 min, rate limit de login, saturación Argon2id acotada, Origin exacto, cookies `HttpOnly`/`Strict`, auditoría append-only) se consideran suficientes **sólo** para el perfil de §5.1. La Beta externa exige §5.2 y aceptación escrita de S1; producción pública exige MFA/step-up.

### 7.4 Aislamiento y persistencia

- Migración desde base vacía: 11 migraciones aplicadas, tablas comerciales y de auditoría presentes.
- Restauración real desde `pg_dump`: conteos (`migrations`, `plan`, `subscription`, `staff_audit_event`) coincidentes con el origen.
- Consultas keyset con `0010` e índices comerciales: `Index Only Scan` con 50 000 eventos de auditoría sintéticos; páginas < 0,15 ms. Sin degradación por paginación profunda.

### 7.5 Dependencias (S8/S18)

- `pnpm audit --prod --audit-level high`: sin hallazgos.
- `cargo audit`: 2 avisos **medios** residuales. Se actualizaron previamente `time` → `0.3.47` y `ctutils` → `0.4.4` para cerrar los dos primeros.

| Advisory | Crate | Severidad | Estado | Justificación |
|---|---|---|---|---|
| RUSTSEC-2026-0459 | `libsqlite3-sys 0.30.1` | 6.8 media | Aceptado | Sólo aparece en `Cargo.lock` como dependencia opcional de `sqlx`; `cargo tree --workspace --all-targets -i libsqlite3-sys` no encuentra ruta de compilación. Platform usa exclusivamente la feature `postgres`. |
| RUSTSEC-2023-0071 | `rsa 0.9.10` | 5.9 media | Aceptado | Igual: dependencia opcional de `sqlx-mysql` sin ruta de compilación. Platform no firma ni verifica RSA; el licenciamiento usa Ed25519. |

**Deuda registrada:** al migrar a `sqlx 0.9` (cambio mayor) desaparecen ambos avisos. No se fuerza el salto en Beta por riesgo de regresión en SQLx/migraciones; se planifica para la fase POS.

### 7.6 Riesgos residuales de la Beta

| Riesgo | Perfil interno | Perfil externo |
|---|---|---|
| MFA pendiente (S1) | Aceptado | Requiere aceptación escrita + compensación §5.2 |
| `support`/`read_only` leen auditoría (S2) | Aceptado | Reducir antes de contratistas |
| `TRUNCATE` no bloqueado por triggers (S6) | Aceptado con backups | Aceptado con backups |
| Sin retención de auditoría (S7) | Aceptado | Aceptado a corto plazo |
| Headers en proxy (S5) | Obligatorio en despliegue | Obligatorio en despliegue |
| Advisories S8/S18 | Aceptados (§7.5) | Aceptados (§7.5) |

Ninguno de estos riesgos es explotable de forma remota y directa en el perfil interno declarado. S5 y la configuración productiva siguen siendo **bloqueantes de despliegue** y se resuelven en el runbook (`docs/03-operations/deployment.md`).

### 7.7 Formulario administrativo de Product Registry (B10.1)

| Aspecto | Verificación |
|---|---|
| Autorización | El alta y la edición sólo se renderizan con `products.manage`; el backend vuelve a comprobarlo y responde `403 forbidden` a `read_only`, verificado por prueba Rust y por E2E directo contra la API. |
| CSRF | Ambas mutaciones pasan por `require_origin`, igual que el resto del Control Center. |
| Validación | Cliente y servidor comparten reglas: clave `[a-z0-9-]` de 1–64 con extremos alfanuméricos, nombre 1–200, descripción ≤ 2000. |
| Duplicados | `409` se traduce a un mensaje neutral, sin detalles de base de datos. |
| Integridad | `key` y `product_type` son inmutables; la edición envía sólo los campos modificados y el backend rechaza no-ops. |
| Auditoría | `product_created` y `product_updated` se escriben en la misma transacción; el E2E los consulta por `resource_id`. |
| Datos | El formulario no expone tokens, identificadores internos ni datos de otras organizaciones. |
