# ADR-0004 — Fronteras del HiQNet Control Center

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Fase:** C0 — Architecture Foundation
**Decisores:** Arquitectura, Producto, Seguridad
**Relacionado:** [ADR-0001](adr-0001-commercial-identity.md), [ADR-0002](adr-0002-authentication-strategy.md), [ADR-0003](adr-0003-installation-enrollment.md)

## 1. Contexto

HiQNet Platform ya dispone de identidad comercial (organizaciones, cuentas, membresías, RBAC comercial), autenticación con sesiones persistidas y enrolamiento seguro de instalaciones. El Customer Portal (`apps/portal`) sirve a `owner/admin/member` de una organización cliente.

El siguiente paso estratégico es crear una superficie **interna** desde la que HiQNet opere todos sus productos y proyectos: el **Control Center**. HiQNet POS será el primer producto conectado, pero no debe condicionar la arquitectura.

## 2. Problema

Sin una frontera explícita, el panel interno tiende a convertirse en:

- una extensión del Customer Portal con permisos elevados;
- el backend administrativo exclusivo del POS;
- un lugar donde se reutilizan endpoints comerciales saltándose el aislamiento por tenant;
- una puerta de operación sin auditoría.

Necesitamos separar identidades, superficies, permisos y contratos desde el primer día.

## 3. Decisión propuesta

El Control Center es una **aplicación y una autoridad de dominio separadas**:

| Superficie                             | Usuarios                     | Identidad               | API                           | Autoridad                          |
| -------------------------------------- | ---------------------------- | ----------------------- | ----------------------------- | ---------------------------------- |
| Customer Portal (`apps/portal`)        | Clientes de una organización | `commercial_account`    | `/api/v1/...` (tenant-scoped) | Consistencia y operación comercial |
| Control Center (`apps/control-center`) | Personal interno HiQNet      | `staff_account` (nueva) | `/api/v1/control/...`         | Operación interna y administración |
| POS local (`hiqnet-pos`)               | Operadores del negocio       | Identidad local POS     | Local-first, SQLite           | Autoridad operativa de venta       |
| Device                                 | Instalaciones                | Clave Ed25519           | `/api/v1/enrollment-*`        | Prueba de posesión de clave        |

Reglas de frontera:

1. El Control Center **no** reutiliza endpoints comerciales para ver datos globales. Crea endpoints staff explícitos con aislamiento en SQL.
2. El Control Center **no** usa `commercialSession` ni `devAdminToken` como identidad.
3. El Control Center **no** es dependencia runtime del POS. El POS sigue siendo local-first.
4. Toda mutación administrativa genera auditoría staff.
5. El núcleo permanece genérico; las reglas específicas pertenecen a módulos de producto.

## 4. Qué NO es el Control Center

- No es el Customer Portal renombrado.
- No es un consola de base de datos productiva.
- No es terminal arbitrario ni botón de despliegue libre.
- No es un lugar para impersonar clientes (soporte) sin diseño específico.
- No aloja secretos en claro por defecto.

## 5. Alternativas evaluadas

- **A. Extender `apps/portal` con roles internos.** Descartada: mezcla identidades, amplía el blast radius de los endpoints comerciales y rompe la separación de superficies.
- **B. Reutilizar el token DEV como login.** Descartada: no representa una cuenta, no audita actor, no permite RBAC ni revocación granular.
- **C. Aplicación + API + identidad separadas (elegida).** Coste inicial mayor, pero escala a N productos y mantiene el blast radius contenido.

## 6. Consecuencias

- Nuevo crate de aplicación/almacenamiento staff y nueva migración (`0005+`) tras aprobar ADR-0005/0006/0007.
- Nuevo namespace de rutas `control` en OpenAPI con security scheme `staffSession`.
- `apps/control-center` nuevo; `apps/portal` permanece intacto.
- Extracción gradual de paquetes compartidos (`ui`, `design-tokens`, `api-client`) cuando exista duplicación real.
- El POS no cambia.

## 7. Invariantes

- Un actor staff nunca se deriva de un ID enviado por el cliente.
- Un endpoint staff nunca acepta `commercialSession` como autoridad.
- Un endpoint staff nunca acepta `devAdminToken` como autoridad.
- Una organización sigue siendo un tenant; una vista global staff no elimina `organization_id` ni constraints.
- Una mutación sin audit event es un defecto, no una omisión aceptable.

## 8. Decisiones ratificadas

- **Namespace API:** `/api/v1/control/...` (`control`).
- **Cookie domain separado** del Customer Portal (no compartir cookie).
- **Alcance C1:** identidad staff + sesiones + `/control/auth/*`; vistas globales y mutaciones administrativas en C2+.
- **Despliegue:** arquitectura documentada; dominio productivo queda pendiente de la fase de despliegue (no se decide aquí).

## 9. Referencias

- [ADR-0005 — Identidad staff](adr-0005-staff-identity.md)
- [ADR-0006 — RBAC staff](adr-0006-staff-rbac.md)
- [ADR-0007 — Product/Project model](adr-0007-product-project-model.md)
- [ADR-0008 — Frontend modular](adr-0008-control-center-frontend.md)
