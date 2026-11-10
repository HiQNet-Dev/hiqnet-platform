# Arquitectura modular de HiQNet Platform

**Estado:** RATIFICADA (refactorización estructural 2026-11-09)
**Relacionado:** [Plataforma](../01-architecture/platform.md) · [Auditoría estructural](../02-development/structural-audit.md) · [ADR-0011](adr-0011-commercial-domain.md)

## 1. Estilo arquitectónico

HiQNet Platform es un **monolito modular con puertos y adaptadores**, organizado por dominios de negocio.

```text
        ┌──────────────── adaptadores de entrada ─────────────────┐
        │  apps/api (Axum)      apps/control-center   apps/portal  │
        └───────────────────────────┬─────────────────────────────┘
                                    │ contratos (puertos)
        ┌───────────────────────────▼─────────────────────────────┐
        │  platform-application   casos de uso, puertos, políticas │
        └───────────────────────────┬─────────────────────────────┘
                                    │
        ┌───────────────────────────▼─────────────────────────────┐
        │  platform-core          dominio: entidades, invariantes  │
        └─────────────────────────────────────────────────────────┘
                                    ▲
        ┌───────────────────────────┴─────────────────────────────┐
        │  platform-storage-postgres   adaptador de salida (SQLx)  │
        │  platform-licensing          firma Ed25519               │
        └─────────────────────────────────────────────────────────┘
```

No hay microservicios, ni framework de inyección, ni un crate por entidad. Los crates existentes se conservan.

## 2. Reglas de dependencia

| Capa | Puede depender de | No puede depender de |
|---|---|---|
| `platform-core` | Sólo de sí mismo y de librerías genéricas | Axum, SQLx, PostgreSQL, React, DTOs HTTP |
| `platform-application` | `platform-core` | Axum, SQLx, detalles de despliegue |
| `platform-storage-postgres` | `platform-core`, `platform-application` | Axum, React |
| `platform-licensing` | `platform-application` | Axum, SQLx |
| `apps/api` | Todos los anteriores | React |
| `apps/*` (web) | Contratos HTTP | Acceso directo a base de datos |

Reglas adicionales:

1. El **dominio no conoce HTTP**. Los DTO viven en el adaptador de entrada.
2. Los servicios de aplicación dependen de **puertos** (`trait`), no de repositorios concretos.
3. La autorización se decide en el backend; la UI sólo refleja capabilities.
4. Los adaptadores de entrada no implementan reglas de negocio: validan transporte y delegan.
5. Un puerto con una sola implementación es aceptable **cuando aísla una infraestructura** (PostgreSQL, cifrado, tiempo). No se crean rasgos decorativos.

## 3. Mapa de módulos

### `platform-core` — dominio

| Módulo | Responsabilidad |
|---|---|
| `staff` | Roles y capabilities |
| `auth` | Sesiones y errores de autenticación |
| `commercial` | Organizaciones, membresías, roles |
| `product` | Productos y proyectos, claves e invariantes |
| `installation` | Identidad de instalación |
| `enrollment` | Protocolo de enrolamiento |
| `license` | `LicenseGrant` v1 y validaciones |
| `plan` | Compatibilidad histórica de licencia |
| `audit` | Tipos de auditoría |

### `platform-application` — casos de uso

| Módulo | Responsabilidad |
|---|---|
| `auth`, `staff` | Autenticación e identidad interna |
| `product`, `control` | Registro y consultas administrativas |
| `audit_security` | Read models C6 |
| `organization_product` | Asociación organización-producto |
| `installation_control` | Suspensión, reactivación y rehost |
| `enrollment` | Inicio, aprobación y proof |
| `commercial_domain` | Planes, suscripciones y entitlements |

### `platform-storage-postgres` — adaptador de salida

Un archivo por agregado, con operaciones agrupadas por responsabilidad y las transacciones intactas.

### `apps/api` — adaptador de entrada y composition root

| Módulo | Responsabilidad |
|---|---|
| `config` | Lectura y validación de variables de entorno |
| `composition` | Ensamblado de servicios, repositorios y estado |
| `router` | Ensamblado de rutas y OpenAPI |
| `health` | Liveness y readiness |
| `commercial_auth` | Login, logout, sesión y contraseña comercial |
| `organizations` | Organizaciones y membresías |
| `licensing` | Rutas legacy/DEV de licenciamiento |
| `control*` | Endpoints del Control Center por dominio |
| `portal_commercial` | Consulta comercial read-only del Portal |
| `tests/` | Pruebas por escenario y fixtures compartidos |

`lib.rs` sólo declara módulos y expone el arranque.

## 4. Política de tamaño y complejidad

| Líneas | Política |
|---|---|
| 0–300 | Normal |
| 301–500 | Aceptable si es cohesivo |
| 501–800 | Revisión recomendada |
| 801–1 000 | Revisión arquitectónica obligatoria |
| > 1 000 | Dividir o documentar una excepción razonada |

Funciones:

- Una responsabilidad por función; extraer pasos con nombre propio antes de añadir ramas.
- Señal de alarma por encima de ~120 líneas o anidamiento mayor de 4 niveles.
- No se fragmenta una transacción para reducir líneas: se nombran sus pasos.

Las excepciones se registran en `scripts/maintenance_baseline.json` con su motivo.

## 5. Verificación automática

`python scripts/check_size_policy.py` inspecciona archivos versionados, excluye generados y lockfiles, y:

1. Informa advertencias con ruta y conteo.
2. Compara contra la línea base: **hace fallar CI** si un archivo crece por encima de su límite o si aparece un archivo nuevo por encima del umbral duro.
3. No falla por la deuda histórica mientras siga igual o menor que la línea base.

La línea base se reduce a medida que se refactoriza; nunca se amplía sin justificación en el mismo PR.

## 6. Lo que no se hace

- No crear un crate por entidad ni un módulo por archivo de 50 líneas.
- No introducir capas intermedias sin una responsabilidad concreta.
- No mover autorización al frontend ni duplicar controles críticos en cada handler.
- No cambiar contratos públicos durante una refactorización estructural.
- No reescribir migraciones publicadas.
