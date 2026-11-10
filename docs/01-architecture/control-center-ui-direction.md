# Control Center — Dirección visual (3 conceptos)

**Estado:** ACCEPTED — Concepto A (Precision) elegido 2026-11-09
**Fecha:** 2026-11-09
**Relacionado:** [ADR-0008](adr-0008-control-center-frontend.md)

Objetivo: una interfaz interna multiproducto, de alta densidad y calidad comparable a Linear/Vercel/Railway/Stripe/GitHub/Cloudflare, con identidad HiQNet propia. No copiar ninguna referencia.

Auditoría previa del Customer Portal: ya existe una base compacta y sobria con azul `#1d4ed8`, tokens en `apps/portal/src/styles/global.css` y componentes reutilizables (`Button`, `Input`, `Badge`, `Dialog`, `DataTable` vía tabla propia, `EmptyState`, `Skeleton`, `ErrorBanner`, `Toast`). El Control Center debe sentirse familiar pero más denso y orientado a operación.

## Concepto A — Precision

- **Idea:** ingeniería serena; el dato manda.
- **Densidad:** alta. Tipografía pequeña y precisa; filas de tabla compactas.
- **Superficies:** fondo casi blanco, bordes 1px neutros, sin sombras salvo elevación de dialog.
- **Color:** neutro dominante; azul HiQNet como acento de acción/foco; estados con color semántico mínimo.
- **Navegación:** sidebar estrecha con iconos + labels; breadcrumb siempre visible.
- **Riesgo:** puede sentirse frío; se compensa con tipografía cuidada y microinteracciones.
- **Encaje HiQNet:** alto (marca técnica, control plane).

## Concepto B — Command

- **Idea:** consola de operaciones; velocidad y contexto.
- **Densidad:** muy alta; tablas y paneles de detalle lado a lado.
- **Superficies:** alto contraste, sidebar oscura opcional, acentos de estado visibles.
- **Color:** acento HiQNet más saturado; badges de estado prominentes.
- **Navegación:** command palette como eje; atajos visibles.
- **Riesgo:** más agresivo; puede envejecer peor y cansar en sesiones largas.
- **Encaje HiQNet:** medio-alto para operadores, pero arriesgado para uso amplio.

## Concepto C — Executive Technical

- **Idea:** producto empresarial con profundidad técnica; equilibrio.
- **Densidad:** media-alta; jerarquía clara; más aire que A/B en cabeceras, denso en tablas.
- **Superficies:** blancas con separadores suaves; una tarjeta sólo cuando agrupa información real.
- **Color:** neutro + azul HiQNet; severidad semántica.
- **Navegación:** sidebar compacta + contexto de producto + command palette.
- **Riesgo:** menor; puede caer en genérico si no se cuida la tipografía.
- **Encaje HiQNet:** alto (sirve a soporte, operación y dirección).

## Comparación

| Criterio                    | A — Precision | B — Command | C — Executive Technical |
| --------------------------- | ------------- | ----------- | ----------------------- |
| Densidad                    | Alta          | Muy alta    | Media-alta              |
| Legibilidad sesiones largas | Alta          | Media       | Alta                    |
| Escalabilidad a N productos | Alta          | Alta        | Alta                    |
| Riesgo de parecer genérico  | Bajo          | Medio       | Medio                   |
| Encaje marca HiQNet         | Alto          | Medio-alto  | Alto                    |

## Recomendación

**Concepto A (Precision) como base**, tomando de C la calidez tipográfica y de B la command palette. Racional: el Control Center es un control plane técnico; la densidad y la precisión son la prioridad, y A es la dirección que envejece mejor y menos se parece a un dashboard genérico.

## Tokens propuestos (compatibles con dark mode futuro)

```
--cc-bg            #fafafa / dark #0b0d10
--cc-surface       #ffffff / dark #13161a
--cc-border        #e5e7eb / dark #262b31
--cc-text          #111827 / dark #e5e7eb
--cc-text-muted    #6b7280 / dark #9aa4b2
--cc-accent        #1d4ed8 / dark #3b82f6
--cc-success       #15803d
--cc-warning       #b45309
--cc-danger        #b91c1c
--radius-sm 4px · --radius-md 6px · --radius-lg 10px
--space-1..8       escala 4/8/12/16/24/32/48/64
```

Tipografía: sans humanista para UI; monoespaciada para IDs, claves y datos técnicos. No mezclar familias decorativas.

## Reglas de implementación visual

- Badges de estado consistentes (mismo estado = mismo color/forma siempre).
- Sin gradientes gratuitos, blobs, glow ni cards decorativas.
- Sin métricas inventadas ni gráficos de relleno.
- Tablas reales con paginación server-side y estados de carga/empty/error.
- Focus visible y navegación por teclado en todas las pantallas.
- Motion sólo para orientación (apertura de nav, dialog, transición de estado).

## Elegido: Concepto A — Precision

Ratificado 2026-11-09: **A — Precision** como base, con la command palette de B y la calidez tipográfica de C. Tokens neutros + azul `#1d4ed8`; dark mode diferido tras C3.
