# Auditoría estructural y política de mantenimiento

**Estado:** R0 completado; métricas finales en §9.
**Fecha:** 2026-11-09
**Alcance:** código escrito manualmente de HiQNet Platform.
**Relacionado:** [Arquitectura modular](../01-architecture/modular-architecture.md) · [Ejecución Beta](platform-beta-execution.md) · [Notas de Beta](platform-beta-notes.md)

## 1. Método

La medición se hizo con un script local sobre `git ls-files`, sin leer archivos ignorados ni artefactos:

- **Líneas físicas:** conteo de saltos de línea + 1.
- **Líneas efectivas:** líneas con contenido que no son comentarios `//`.
- **Funciones:** heurística de declaración (`fn`, `function`, `const … = (`), emparejando la llave de cierre.
- **Función más larga:** el mayor intervalo declaración–cierre detectado.
- **Bytes:** tamaño en UTF-8 del archivo versionado.

Exclusiones aplicadas (no se fragmentan artificialmente):

```text
node_modules/ · target/ · openapi/openapi.json · **/generated/** · *.d.ts
pnpm-lock.yaml · Cargo.lock · migrations/**
```

Las migraciones históricas son inmutables y requieren controles propios, no división.

## 2. Línea base

| Métrica | Valor |
|---|---|
| Archivos de código versionados y medidos | 166 |
| Archivos > 1 000 líneas | **6** |
| Archivos entre 801 y 1 000 | **4** |
| Archivos entre 501 y 800 | **6** |
| Archivo manual más largo | `apps/api/src/lib.rs` · **6 109 líneas** |
| Funciones de más de 400 líneas | **3** |
| Dependencias circulares entre crates | **0** |
| `cargo test --workspace` | 150 passed · 0 failed · 1 ignored |
| Control Center | 20 unitarias · 180 E2E simuladas · 15 E2E full-stack |
| Portal | 9 unitarias · 4 E2E full-stack |
| Build incremental de la crate API | 6,9 s |

## 3. Ranking de archivos problemáticos

### 3.1 Crítico

| Archivo | Líneas | Funciones | Problema | Decisión |
|---|---|---|---|---|
| `apps/api/src/lib.rs` | 6 109 | 90 | 1 452 líneas de producción + 4 655 de pruebas; mezcla composición, router, handlers legacy, autenticación comercial, organizaciones, errores y `run()` | **Dividir** (R1) |

Responsabilidades detectadas en `lib.rs`:

| Rango | Responsabilidad |
|---|---|
| 1–52 | Imports y `AppState` |
| 58–146 | Mapeo de errores y respuestas |
| 147–272 | Ensamblado de rutas OpenAPI |
| 273–327 | `build_router`, `health`, `ready` |
| 328–484 | Endpoints legacy/DEV de licenciamiento |
| 485–567 | Traducción de errores de autenticación |
| 568–901 | Autenticación comercial (login/logout/me/password/sessions) |
| 902–949 | Mapeo de organizaciones y membresías |
| 950–1238 | Organizaciones y membresías |
| 1239–1452 | `run()`: configuración, composición y `serve` |
| 1453–6108 | 53 pruebas y sus fixtures |

Riesgo de regresión: **alto** (superficie HTTP completa). Esfuerzo: **alto**. Estrategia: extracción mecánica sin reescribir lógica, verificando compilación y pruebas después de cada bloque.

### 3.2 Alto

| Archivo | Líneas | Problema | Decisión |
|---|---|---|---|
| `crates/platform-storage-postgres/tests/installation_control.rs` | 2 627 | 33 pruebas de instalaciones, rehost y concurrencia; la mayor mide 384 líneas | **Dividir** por escenario (R4) |
| `crates/platform-storage-postgres/tests/commercial.rs` | 1 655 | Organizaciones, membresías y roles en un solo archivo | **Dividir** (R4) |
| `crates/platform-storage-postgres/src/commercial_domain.rs` | 1 024 | Un `impl CommercialRepository` de 743 líneas con planes, suscripciones, transiciones, entitlements y consultas del Portal | **Dividir** (R2) |
| `crates/platform-storage-postgres/src/enrollment.rs` | 1 015 | `complete_proof` de 481 líneas; transacción criptográfica sensible | **Reorganizar por pasos internos** sin romper la transacción (R3) |
| `crates/platform-storage-postgres/tests/auth.rs` | 1 129 | Autenticación, sesiones, bloqueo y concurrencia | **Dividir** (R4) |

### 3.3 Medio

| Archivo | Líneas | Problema | Decisión |
|---|---|---|---|
| `crates/platform-application/src/commercial_domain.rs` | 911 | Modelos, puerto y servicio con planes, suscripciones y entitlements en un solo archivo | **Dividir** (R2) |
| `apps/api/src/control_commercial.rs` | 901 | DTOs y endpoints de planes, suscripciones y entitlements | **Dividir** (R2) |
| `crates/platform-application/src/audit_security.rs` | 818 | Modelos, filtros, cursor y servicio C6 | Mantener: cohesión alta, funciones cortas (máx. 42) |
| `apps/api/src/control_audit.rs` | 713 | Seis endpoints y DTOs de auditoría/seguridad | Mantener: cohesión por dataset, funciones cortas |
| `crates/platform-storage-postgres/src/commercial.rs` | 703 | Organizaciones y membresías; `list_members` de 116 líneas | Mantener con excepción documentada (R2 lo revisa) |
| `apps/api/src/control.rs` | 602 | Productos, proyectos y sus errores | Mantener: por debajo de 800 y cohesivo |
| `crates/platform-storage-postgres/src/installation_control.rs` | 602 | `authorize_rehost` de 305 líneas | **Reorganizar** (R3) |
| `crates/platform-storage-postgres/src/auth.rs` | 519 | Credenciales, sesiones y bloqueo | Mantener: cohesivo y bajo 800 |

### 3.4 Frontend

| Archivo | Líneas | Problema | Decisión |
|---|---|---|---|
| `apps/control-center/src/features/installations/installation-detail-page.tsx` | 481 | Página, diálogos y helpers de error en un archivo | **Mantener**: bajo el umbral de 500 y cubierto por E2E; la extracción de diálogos no reduce acoplamiento real |
| `apps/control-center/src/features/commercial/subscriptions-page.tsx` | 422 | Lista + detalle + dos formularios | **Mantener**: bajo 500; el detalle comparte estado con la lista |
| `apps/control-center/src/features/organizations/organizations-page.tsx` | 375 | Lista + detalle | Mantener con excepción documentada |
| `apps/portal/src/features/enrollment/EnrollmentPage.tsx` | 323 | Flujo de enrolamiento con estados | Mantener: bajo 500 y cohesivo |

## 4. Mapa de dependencias

```text
platform-core            (dominio puro: entidades, invariantes, errores)
      ↑
platform-application     (casos de uso + puertos)
      ↑              ↑
platform-licensing   platform-storage-postgres (adaptador de salida SQLx)
      ↑              ↑
            apps/api        (adaptador de entrada HTTP + composition root)
              ↑
      control-center / portal (adaptadores de entrada web)
```

Verificado por `Cargo.toml`:

- `platform-core` no depende de ningún crate interno.
- `platform-application` sólo depende de `platform-core`.
- `platform-storage-postgres` depende de `platform-core` y `platform-application`.
- `platform-licensing` depende de `platform-application`.
- `apps/api` es el único que conoce todos los anteriores.

**Dependencias circulares: ninguna.** La regla hexagonal ya se cumple a nivel de crates; el problema es *intra-crate*: archivos que mezclan capas.

## 5. Duplicación y acoplamiento detectados

1. **Mapeo de errores repetido en handlers** de `lib.rs`, `control.rs`, `control_commercial.rs`, `control_audit.rs` y `control_installations.rs`. Cada uno define su `fn …_error(...)`. Es duplicación legítima por dominio, pero conviene centralizar el patrón común (`HttpError` + `ErrorResponse`).
2. **Acceso a estado staff** repetido (`actor(&auth_state, &context).await?` + `require_origin`). Debe seguir siendo explícito en cada handler: es el control de seguridad, no un boilerplate a ocultar.
3. **DTOs de comercial** mezclados con endpoints en `control_commercial.rs`; separables sin tocar rutas.
4. **Pruebas sin sección por escenario**: un archivo de 2 627 líneas obliga a buscar por nombre.
5. **Funciones largas por transacción**: `complete_proof` y `authorize_rehost` acumulan pasos secuenciales. Se pueden nombrar los pasos sin partir la transacción.

## 6. Candidatos a extracción con riesgo bajo

- Handlers legacy de licenciamiento → módulo propio (no los usa ningún frontend).
- `health`/`ready` → módulo de operación.
- Autenticación comercial → módulo propio (ya existe `auth.rs`; no duplicar).
- Organizaciones y membresías → módulo propio.
- Configuración por entorno → `Settings::from_env()`.
- Composición de servicios → función de composición explícita.

## 7. Plan de incrementos

| Incremento | Alcance | Riesgo | PR |
|---|---|---|---|
| R0 | Este informe y la arquitectura objetivo | Nulo | Documentación |
| R1 | `apps/api/src/lib.rs` + `apps/api/src/tests/` | Alto | Refactor API |
| R2 | Dominio comercial: aplicación, persistencia y HTTP | Medio | Refactor comercial |
| R3 | Enrolamiento y control de instalaciones | Alto (cripto y transacciones) | Refactor enrolamiento |
| R4 | Pruebas de persistencia por escenario | Medio | Refactor pruebas |
| R5 | Frontend, verificador de tamaño y CI | Bajo | Refactor frontend + control |

Regla aplicada a todos: **sin cambios de comportamiento**. Se preservan rutas, DTOs, operation IDs, estados, transacciones, bloqueos, idempotencia y OpenAPI.

## 8. Criterios de aceptación

- `apps/api/src/lib.rs` deja de contener miles de líneas de pruebas y se centra en composición.
- El dominio comercial tiene límites reconocibles (modelo, puertos, planes, suscripciones, entitlements).
- El adaptador PostgreSQL se organiza por operación, conservando transacciones y bloqueos.
- El enrolamiento conserva atomicidad y protocolo Ed25519.
- Las pruebas se localizan por escenario.
- Sin dependencias circulares nuevas ni abstracciones decorativas.
- OpenAPI y contratos intactos.
- Gates verdes y E2E full-stack aprobados.
- Verificador de tamaño activo en CI.

## 9. Métricas finales

Medición con el mismo script y las mismas exclusiones que la línea base, tras R1–R5.

| Métrica | Antes | Después |
|---|---|---|
| Archivos > 1 000 líneas | **6** | **0** |
| Archivos entre 801 y 1 000 | **4** | **2** (ambos con excepción registrada) |
| Archivos entre 501 y 800 | 6 | 11 (antes había 6 archivos de más de 1 000 que ahora son varios de tamaño medio) |
| Archivo manual más largo | **6 109** (`apps/api/src/lib.rs`) | **901** (`apps/api/src/control_commercial.rs`) |
| Archivos medidos | 166 | 221 (módulos por responsabilidad, no crates nuevos) |
| Dependencias circulares | 0 | **0** |
| Pruebas Rust | 150 passed / 1 ignored | **150 passed / 1 ignored** |
| E2E Control Center full-stack | 15/15 | 15/15 |
| E2E Portal full-stack | 4/4 | 4/4 |
| OpenAPI | 55 rutas / 110 esquemas | **idéntico byte a byte** |
| Build incremental de la crate API | 6,9 s | 7,8 s (una muestra; sin cambio material) |
| Defectos nuevos detectados | 0 | **0** |

Funciones de más de 200 líneas: 11 archivos las contenían antes y 17 después. El número no mejora porque **dividir archivos no divide funciones**: la mayoría son escenarios de prueba lineales, y las dos funciones de producción largas (`complete_proof` 480 y `authorize_rehost` 305) se mantienen deliberadamente por ser secuencias transaccionales y criptográficas únicas.

### Excepciones registradas

| Archivo | Líneas | Motivo |
|---|---|---|
| `apps/api/src/control_commercial.rs` | 900 | DTOs y endpoints de un mismo contrato; funciones de máximo 37 líneas |
| `crates/platform-application/src/audit_security.rs` | 817 | Read models C6 cohesivos con whitelist de metadata |

`crates/platform-storage-postgres/src/commercial_domain/repository.rs` (750) y `installation_control.rs` (601) no son excepciones: están por debajo del umbral de revisión obligatoria, aunque el primero no puede dividirse por ser una implementación de trait.

## 10. Regla preventiva

`python scripts/check_size_policy.py` corre en CI (job `Archivos prohibidos`) y falla si:

- aparece un archivo de más de 1 000 líneas sin excepción registrada;
- un archivo crece por encima de su línea base.

Las advertencias de 500–1 000 líneas se imprimen para revisión, sin romper CI. La línea base (`scripts/maintenance_baseline.json`) sólo se reduce.
