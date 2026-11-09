# ADR-0008 — Arquitectura modular del frontend del Control Center

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Fase:** C0 — Architecture Foundation
**Relacionado:** [ADR-0004](adr-0004-control-center-boundaries.md), [ADR-0007](adr-0007-product-project-model.md)

## 1. Contexto

El Control Center debe ser multiproducto y modular. No debe contener un `if (product === "pos")` disperso, ni un sidebar global que crezca con cada función de cada producto. Al mismo tiempo, no queremos un sistema de plugins dinámico ni microfrontends en esta fase.

## 2. Decisión propuesta

### 2.1 Aplicación separada

```
apps/
  api/
  portal/            # Customer Portal (existente, intacto)
  control-center/    # nuevo

packages/            # sólo cuando exista duplicación real
  ui/
  design-tokens/
  api-client/
```

No se copia `apps/portal` cambiando textos. Se comparten componentes extraídos sólo cuando Portal y Control Center realmente los necesiten (inventario previo).

### 2.2 Registry estático tipado

```ts
interface ControlCenterModule {
  id: string;
  label: string;
  route: string;
  icon: Icon;
  requiredCapability?: Capability;
}

interface ProductModule {
  productKey: string;
  navigation: ControlCenterModule[];
  routes: RouteDefinition[];
}
```

Un **registry estático** (no plugins en runtime) compone navegación y rutas por producto. Añadir un producto nuevo = registrar un módulo, sin tocar `AppShell`.

### 2.3 Estructura orientativa

```
apps/control-center/src/
  app/            # bootstrap, router, providers
  api/            # controlClient (separado del comercial)
  auth/           # staff session bootstrap
  components/     # componentes propios del Control Center
  layouts/        # AppShell, ProductLayout
  modules/        # registry global
  features/
    overview/
    products/
    organizations/
    accounts/
    security/
    settings/
  styles/         # tokens
```

### 2.4 Rutas

```
/control
/control/products
/control/products/:productKey
/control/products/:productKey/installations
/control/organizations
/control/organizations/:id
/control/accounts
/control/security/audit
/control/settings
```

En dominio propio `/control` puede omitirse; no se decide dominio productivo en esta fase.

### 2.5 Server state

- TanStack Query con keys namespaced:
  - `["control","products"]`
  - `["control","product",productKey]`
  - `["control","organizations", filters]`
- Nunca keys globales compartidas con el Customer Portal.
- Sin stores globales grandes; estado local del componente por defecto.

### 2.6 API clients separados

- `controlClient` (security scheme `staffSession`) y `commercialClient` (`commercialSession`) generados desde el mismo OpenAPI, pero **sin mezclar** autorización por conveniencia.
- Ambas apps siempre `credentials: "include"`.

### 2.7 AppShell

Incluye sidebar global, context switcher, breadcrumbs, disparador de búsqueda/command palette, acciones de página, menú de usuario e indicador de entorno. Sin ruido visual.

### 2.8 Command palette (arquitectura, no implementación completa)

`⌘K / Ctrl+K` abre un palette que en el futuro permite abrir producto, organización, instalación y navegar. Las acciones destructivas no se exponen inicialmente.

## 3. Consecuencias

- CI gana un job de frontend para `control-center` (lint, typecheck, test, build, `api:check`).
- Se añade `apps/control-center` a `pnpm-workspace.yaml` sólo al implementarlo (C3).
- Extracción de `packages/ui`/`design-tokens` se hace con inventario y sin abstracción prematura.

## 4. Decisiones ratificadas

- **Registry estático tipado** (no generado desde backend).
- **React Router** (ya usado en Portal).
- **Paquetes compartidos:** se duplican temporalmente; se extraen a `packages/ui`/`design-tokens` sólo cuando exista duplicación real.
- **Dark mode:** tokens compatibles desde el inicio; implementación visual después de C3.

## 5. No objetivos

- Microfrontends.
- Plugins en runtime cargados desde servidor.
- Reconstruir todo el design system antes de usarlo.
