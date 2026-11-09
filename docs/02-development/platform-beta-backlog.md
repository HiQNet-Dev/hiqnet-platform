# HiQNet Platform — Backlog cerrado de Beta 0.1.0

**Estado:** PROPUESTO — pendiente de ratificación (Fase 1, 2026-11-09)
**Alcance:** Beta administrativa web de Platform
**Relacionado:** [Arquitectura rectora](../01-architecture/platform.md) · [ADR-0011](../01-architecture/adr-0011-commercial-domain.md) · [Seguridad Beta](platform-beta-security.md) · [Roadmap](../01-architecture/control-center-roadmap.md)

> Este documento propone cerrar el alcance de Platform Beta y queda pendiente de ratificación. Nada P2/P3 se promueve sin demostrar que bloquea un criterio de aceptación. No autoriza por sí mismo merges, migraciones, tags, releases ni despliegues.

## 1. Definición de Beta

Platform Beta 0.1.0 es una plataforma administrativa **web** que permite a HiQNet:

1. autenticar personal staff;
2. administrar productos, proyectos, organizaciones y cuentas;
3. habilitar productos para organizaciones;
4. definir planes por producto;
5. asignar y transicionar suscripciones manualmente;
6. resolver derechos efectivos;
7. consultar instalaciones cuando el producto las use;
8. ofrecer un Customer Portal para que un cliente consulte y administre sus recursos, incluidos productos contratados, plan, suscripción y entitlements relevantes en modo read-only;
9. consultar auditoría y seguridad;
10. operar el flujo completo con PostgreSQL real, no sólo mocks.

No incluye pagos, facturación, C7, sync cloud, apps nativas ni control remoto.

## 2. Matriz definitiva de módulos

| Módulo | Estado | Decisión Beta | Evidencia / acción |
|---|---|---|---|
| `platform-core` | CONSERVAR / ADAPTAR | Mantener; añadir tipos de dominio comercial, sin acoplar a POS | `plan.rs` se conserva para compatibilidad de licencia; el catálogo pasa a datos |
| `platform-application` | CONSERVAR / COMPLETAR | Añadir servicios de planes, suscripciones, entitlements y C6 read-side | C1–C5 intactos |
| `platform-storage-postgres` | CONSERVAR / COMPLETAR | Añadir persistencia comercial y consultas C6 | SQLx incremental |
| `platform-licensing` | CONSERVAR / ADAPTAR | Mantener Ed25519 v1; emisión productiva diferida | No cambiar `LicenseGrant` v1 |
| `apps/api` | CONSERVAR / COMPLETAR | Añadir endpoints comerciales, OrganizationProduct y C6 | OpenAPI aditivo |
| `apps/control-center` | CONSERVAR / COMPLETAR | Añadir Plans, Subscriptions, OrgProduct, Audit, Security | Reutilizar shell/estilos |
| `apps/portal` | COMPLETAR | Integrar PR #12 y #13 | Rebase + conflictos CI/workspace |
| Migraciones `0001–0009` | CONSERVAR | No reescribir | Incrementales |
| Auth staff/comercial | CONSERVAR | No rehacer | Cookies y sesiones separadas |
| RBAC | CONSERVAR | Reutilizar capabilities; sin nuevas en Beta | Ver §5 |
| Auditoría/seguridad | COMPLETAR | Implementar C6.1–C6.4 | Contrato PR #22 |
| OpenAPI | CONSERVAR / COMPLETAR | Contratos aditivos y snapshot | Mismo PR que cada endpoint |
| CI | CONSERVAR / ADAPTAR | Integrar Portal job y workspace | Resolver conflicto PR #12 |
| Docs/ADRs | CONSERVAR / COMPLETAR | Fase 1 + runbook Beta | Este documento y ADR-0011 |
| POS/Tauri/Android | FUERA DE PLATFORM | No introducir | POS se trabaja en Fase 8 |

No hay módulos RETIRAR.

## 3. Backlog cerrado

### P0 — Bloqueantes de Beta

| ID | Incremento | Entregable | Criterio de aceptación |
|---|---|---|---|
| B0 | Fase 1 documental | `platform.md`, ADR-0011, backlog, seguridad, enmiendas | Documentos revisados y aprobados |
| B1 | Portal F01 | Rebase de PR #12 sobre `main`; conflictos CI/workspace resueltos | Portal en `main`; CI portal+rust+control verdes |
| B2 | Portal F02 | Rebase de PR #13 sobre F01; merge | Enrolamiento e instalaciones en Portal; E2E contra API real |
| B3 | C6.1 backend | Migración `0010` (índices), read models, endpoints Audit/Security/Staff Auth, OpenAPI, tests | Contrato ADR-0010 implementado; RBAC y sanitización verdes |
| B4 | OrganizationProduct | Backend activate/deactivate + UI en Control Center | `organizations.manage`; auditoría; sin DELETE; UI con estados |
| B5 | C6.2 frontend | Módulos Audit y Security (Platform/Staff Auth) | Rutas, filtros, detalle, 401/403/404, 5 viewports |
| B6 | Dominio comercial | Migración `0011+`: plan, subscription, subscription_event, entitlements; backend | ADR-0011 implementado; transiciones auditadas, trial único, expiración lógica, plan inmutable con subscriptions e idempotencia |
| B7 | UI comercial | Plans y Subscriptions en Control Center | Crear plan, asignar suscripción, suspender/reactivar/cancelar/renovar |
| B7b | Customer Portal comercial read-only | API `/organizations/{id}/products` y vista Portal con plan/suscripción/entitlements | Membresía activa, 404 neutral, sin endpoints staff ni datos de otras organizaciones |
| B8 | Escenario E2E full-stack | Pasos de §4 con frontend, Axum y PostgreSQL reales | Escenario §4 completo; aislamiento cross-tenant |
| B9 | Validación Beta | Migración limpia, backup/restore, seguridad, rendimiento, E2E | Checklist §5 y seguridad §5.1/§5.2 verdes |
| B10 | Cierre documental | README, runbook deploy/reversión, notas de Beta, transición POS | Revisión y autorización de release |

### P1 — Necesarios para la calidad de Beta

| ID | Incremento | Entregable |
|---|---|---|
| B11 | Segundo producto sintético | Producto no-POS con plan, suscripción y entitlements; sin instalaciones |
| B12 | Evidencia Playwright | MCP si está disponible; si no, CLI con evidencia documentada |
| B13 | Seguridad de despliegue | TLS, headers, configuración productiva, secretos, backups; perfiles interno/externo |
| B14 | Escaneo de dependencias/secretos | `cargo audit`/`pnpm audit` o decisión documentada |
| B15 | Docs de Portal | `docs/02-development/portal.md` actualizado en `main` |

### P2 — No bloqueantes

- Staff management endpoints/UI (`staff.manage`).
- Portabilidad de `api:check` en Windows (CRLF).
- Filtros y pulido visual adicional.
- Contrato de licensing productivo y custodia de clave.
- Retención/particionado de auditoría.
- MFA/step-up.
- Mutaciones staff de organizaciones/cuentas si operaciones las requiere.

### P3 — Posponer

- Pagos, checkout, cobros, conciliación, facturación fiscal, CFDI.
- C7 environments/releases/deployments.
- Sync cloud.
- Plugins dinámicos, microservicios, microfrontends.
- Apps nativas/PWA offline.
- Control remoto, SQL remoto, filesystem, impersonación.
- SIEM, risk score, severity engine, geolocation, device reputation.
- Incident management y respuesta automática.
- Detección fiable de clones.

## 4. Escenario de aceptación principal

Con datos persistentes de prueba y servicios reales:

1. Un administrador staff registra un producto genérico (`demo-service`, no POS).
2. Se registra una organización comercial.
3. Se asocia el producto con la organización (`OrganizationProduct active`).
4. Se configura un plan del producto con features/límites.
5. Se asigna una suscripción administrada manualmente.
6. Se verifican los derechos efectivos (`active` → features; `suspended` → sin derechos; `inactive` org-product → sin derechos).
6b. Si se prueba un trial, se verifica que no puede repetirse para el mismo par organización-producto y que al expirar deja de conceder derechos aunque el `status` persistido siga `active`.
7. Se registra o vincula una instalación **sólo si el producto lo permite** (para `demo-service`, el flujo debe funcionar sin instalación; para `pos`, con instalación).
8. El administrador consulta todo desde Control Center.
9. Un cliente autorizado consulta sus productos contratados, plan, estado de suscripción y entitlements desde Customer Portal.
10. Otro cliente intenta acceder a la organización ajena y recibe rechazo neutral.
11. Las mutaciones relevantes dejan auditoría staff y eventos de seguridad.
12. Los resultados persisten tras reiniciar API y PostgreSQL.

El escenario se ejecuta con API Axum + PostgreSQL reales. Los mocks no son evidencia suficiente.

## 5. Criterios de cierre de Beta

### Funcional

- [ ] Control Center autentica staff.
- [ ] Product Registry funciona.
- [ ] Organizaciones y cuentas se consultan correctamente.
- [ ] OrganizationProduct se activa/desactiva con auditoría.
- [ ] Installation Management funciona según C5.
- [ ] Planes y suscripciones se administran manualmente.
- [ ] Entitlements efectivos se resuelven correctamente.
- [ ] Customer Portal autentica y consulta recursos propios.
- [ ] Customer Portal muestra productos contratados, plan, estado de suscripción y entitlements sin datos de otras organizaciones.
- [ ] Enrolamiento validado en entorno de pruebas.
- [ ] Audit/Security ofrecen consultas mínimas.
- [ ] Un producto no-POS funciona sin instalaciones/licencias offline.
- [ ] Un cliente ajeno no puede acceder a otra organización.

### Técnico

- [ ] `cargo fmt`, `clippy`, `cargo test` verdes con PostgreSQL 17 real.
- [ ] Frontends: lint, typecheck, unit, build, api:check y E2E verdes.
- [ ] OpenAPI consistente y snapshots actualizados.
- [ ] Migraciones desde base limpia e incremental validadas.
- [ ] Backup/restore sintético validado.
- [ ] RBAC y aislamiento tenant comprobados.
- [ ] Sin vulnerabilidades críticas conocidas ni defectos bloqueantes.
- [ ] Evidencia Playwright (MCP o CLI documentada).
- [ ] Interfaces responsive en 5 viewports.
- [ ] Documentación y runbook actualizados.
- [ ] Versión candidata validada en entorno web representativo.

### Seguridad

- [ ] Configuración productiva: `ENV=production`, cookies `Secure`, `SameSite` correcto, Origin exacto.
- [ ] `HIQNET_PLATFORM_ADMIN_TOKEN` vacío en producción.
- [ ] `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE=false`.
- [ ] `/docs` deshabilitado salvo decisión explícita.
- [ ] TLS y headers de seguridad en el proxy/static host.
- [ ] Sin emisión productiva de licencias sin custodia de clave aprobada.
- [ ] Metadata de auditoría/seguridad saneada.
- [ ] Riesgos residuales aceptados y documentados.

## 6. Secuencia de PRs y dependencias

| Orden | PR | Depende de | Toca migración | Riesgo de conflicto |
|---|---|---|---|---|
| 0 | PR #22 C6 contract merge | Revisión/aprobación | No | Docs |
| 1 | B1 Portal F01 rebase | `main` + PR #22 mergeado | No | CI/workspace/lock |
| 2 | B2 Portal F02 | B1 | No | Bajo |
| 3 | B3 C6.1 backend + `0010` | PR #22 merge | `0010` | API/OpenAPI |
| 4 | B4 OrganizationProduct | B3 (OpenAPI base) | No | API/Control Center |
| 5 | B5 C6.2 frontend | B3 | No | Control Center |
| 6 | B6 Commercial backend + `0011+` | B3/B4/ADR-0011 | `0011+` | API/OpenAPI |
| 7 | B7 Commercial UI | B4/B6 | No | Control Center |
| 7b | B7b Portal comercial read-only | B6/B7/B2 | No | Portal/API |
| 8 | B8 E2E full-stack | B2/B5/B7/B7b | No | Bajo |
| 9 | B9 Validación + docs | B8 | No | Bajo |
| 10 | B10 Cierre/release | B9 + autorización | No | Bajo |

Orden de dependencia confirmado: **PR #22 → B1 → B2 → B3 → B4 → B6 → B5/B7 → B7b → B8 → B9 → B10**. B5 (C6.2 frontend) puede ejecutarse en paralelo con B4 tras B3; B6 no se adelanta a B4.

PR #22 (contrato C6) no es dependencia técnica de B1/B2; se ordena antes para estabilizar el contrato. Si PR #22 se retrasa, B1/B2 pueden avanzar sin C6.

Reglas:

- Un PR no mezcla migración, backend, frontend y docs salvo que el incremento sea indivisible.
- Cada PR actualiza OpenAPI y tests en el mismo cambio.
- No se reescriben migraciones publicadas.
- No se hace merge de B10 sin autorización separada.

## 7. Dependencias de migraciones

```text
0001–0009  existentes (inmutables)
0010       C6 audit/security read indexes        (B3)
0011+      commercial domain                     (B6)
```

Si B6 se adelanta a B3:

- renumerar C6 a `0011`;
- actualizar ADR-0010 y PR #22 antes de implementar;
- no ejecutar dos numeraciones en paralelo.

## 8. Riesgos y bloqueos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| PR #12 conflictúa en CI/workspace/lock | Portal no integrable | Rebase temprano; resolver 4 archivos; revalidar |
| OrganizationProduct sin mutación | Escenario bloqueado | B4 antes de B8 |
| Dominio comercial inexistente | Escenario bloqueado | ADR-0011 + B6/B7 |
| Planes POS acoplados al núcleo | Contaminación multiproducto | Planes como datos; seed POS; segundo producto |
| Emisión de licencias DEV | Riesgo de confusión productiva | Mantener deshabilitada; contrato posterior |
| C6 no implementado | Checklist incompleto | B3/B5 |
| `api:check` CRLF en Windows | Gate local degradado | Normalizar comparación (P2) |
| Sin retención de auditoría | Crecimiento | Política posterior (P3) |
| Sin MFA/headers | Hardening | Condiciones de seguridad Beta |
| Muchas ramas históricas | Ruido y conflictos | Higiene post-Beta |

## 9. Transición a HiQNet POS

Al cerrar Beta:

1. Congelar nuevas funcionalidades de Platform.
2. Mantener sólo parches de seguridad, defectos e integración POS.
3. Auditar POS (Rust/React/SQLite, ventas, inventario, caja, compras, gastos, merma, devoluciones, backups, offline).
4. Implementar el recorrido POS ↔ Platform:
   - identidad local;
   - enrolamiento;
   - binding con organización;
   - proof criptográfico;
   - derechos comerciales;
   - licencia firmada;
   - operación offline;
   - renovación;
   - reinstalación/rehost;
   - datos locales preservados.
5. La sincronización cloud es un frente separado; no es requisito oculto de Platform Beta.
6. No introducir dependencia runtime de Platform para vender.

## 10. Fuera de alcance

- Pagos, facturación, CFDI, conciliación.
- C7 environments/releases/deployments.
- Sync cloud.
- Administración de repositorios/pipelines.
- Terminal remoto, SQL remoto, filesystem.
- Impersonación de clientes.
- SIEM, risk score, severity engine, geolocation, device reputation.
- Incident management.
- Detección fiable de clones.
- Plugins dinámicos, microservicios, microfrontends.
- Apps nativas/PWA offline.

## 11. Versionado y release Beta

El esquema vigente es `scripts/validate_release.py` + `.github/workflows/release-platform.yml`:

- `Cargo.toml` workspace version: `0.1.0`.
- Tag válido: `platform-v<SemVer>`; el prefijo del tag debe coincidir con la versión del workspace.
- `workflow_dispatch` sólo simula (valida Rust y construye imagen local sin publicar); un tag `platform-v*` dispara la validación, el build de imagen y un GitHub Release. El workflow **no** hace push de la imagen Docker.
- No se admite metadata `+build`.

**Identificación Beta:** usar un prerelease SemVer:

```text
Versión: 0.1.0-beta.1
Tag:     platform-v0.1.0-beta.1
prerelease: true
```

- Mientras el workspace siga en `0.1.0`, el tag `0.1.0-beta.N` es válido porque el prefijo coincide.
- La simulación correcta es:

```bash
RELEASE_EVENT=workflow_dispatch \
RELEASE_REF=refs/heads/main \
RELEASE_VERSION=0.1.0-beta.1 \
python scripts/validate_release.py
# tag=platform-v0.1.0-beta.1
# prerelease=true
```

- La simulación con `0.1.0` (`prerelease=false`) corresponde a una versión final, no a la Beta.
- La promoción a final (`0.1.0`, `prerelease=false`) requiere validación y autorización separadas.
- No se crea tag ni release en Fase 1 ni en los PRs de implementación. El tag Beta se crea sólo en B10, tras B9 y autorización explícita.
