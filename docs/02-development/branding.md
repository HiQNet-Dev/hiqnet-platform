# Identidad visual de HiQNet Platform

Criterios de marca y sistema visual para **Control Center** y **Customer Portal**.
Esta identidad pertenece exclusivamente a HiQNet Platform: **HiQNet POS conserva
su propia identidad** y no consume estos tokens ni recursos.

## Fuente de verdad

Los recursos originales provienen del repositorio corporativo
`HiQNet-Dev/hiqnet.github.io`:

| Recurso | Uso |
|---|---|
| `hiqnet-logo.webp` (1850×550) | Logotipo horizontal: sidebar, login y cabeceras |
| `hiqnet-mark.webp` / `.avif` (360×360) | Isotipo orbital: contextos compactos y avatares de marca |
| `favicon.png`, `apple-touch-icon.png` | Navegador y dispositivos |

Reglas:

- Usar siempre el recurso original; no reconstruirlo en CSS/SVG ni aproximarlo.
- Escalar **solo por altura** (`height` + `width: auto`) para conservar la
  proporción; nunca estirar ni recortar.
- Mantener el degradado cian→azul→violeta→magenta del logotipo como elemento de
  identidad, no como decoración de botones, títulos o tarjetas.
- En contenedores flex en columna, fijar `align-self: flex-start` en la marca
  para evitar el estiramiento transversal.

## Tokens

Definidos en `apps/<app>/src/styles/tokens.css` (Control Center) y en el bloque
raíz de `apps/portal/src/styles/global.css` (Portal, sobre las variables
existentes).

| Grupo | Valores |
|---|---|
| Fondos | `#0D0F12` (principal), `#07090B` (profundo) |
| Superficies | `#0F1217`, `#151922` (elevada), `#0C0F13` (inset: inputs y tablas) |
| Texto | `#ECEEF2`, `#A5AFBD` (secundario), `#7D8594` (sutil) |
| Bordes | `#232B35`, `#191F27` (suave), `#343B45` (fuerte) |
| Acento | `#2AD1F2`, fuerte `#8CEAFF`, tenue `#2094B0`, tinta `#03171C` |
| Foco | `#58DCF5` |
| Peligro | `#F2685C` |

Variaciones semánticas justificadas para estados administrativos (no forman una
segunda paleta): éxito `#52D1A4`, aviso `#E6B45A`, info = acento. Todas se usan
con fondos tintados de baja opacidad y texto de alto contraste (WCAG AA).

Escalas: espaciado `--space-1..8`, radios `3/5/8/12px` y píldora, sombras
`panel`/`overlay`, movimiento `120/180/320ms` con `ease-out`, breakpoints
`56rem`, `48rem`, `25rem`.

## Tipografía

- **Space Grotesk** (variable, autoalojada): títulos, encabezados y jerarquía.
- **Inter** (variable, autoalojada): cuerpo, formularios, tablas, navegación y
  datos operativos.
- Carga con `@font-face` apuntando a `@fontsource-variable` (subset latin);
  sin CDNs externos. `font-display: swap`.

## Componentes

Tratamiento compartido: sidebar de marca, topbar con breadcrumbs y usuario,
botones (primario acento con tinta oscura, secundario delineado, fantasma,
peligro), inputs/selects/textareas (inset + foco con anillo), tablas densas con
cabecera en versalitas, badges semánticos, paneles, notas informativas, diálogos
con backdrop difuminado, estados vacío/carga/error y skeleton.

Estados obligatorios: normal, hover, focus-visible, disabled, loading y error.
Iconografía de línea coherente; no se usan emojis como iconos de interfaz.

Criterios de composición aplicados en las fases C y D:

- **Un solo `h1` por pantalla.** Las secciones dentro de una organización usan
  `PageHeader level={2}` y una escala menor; la cabecera de la organización
  conserva la jerarquía principal.
- **Fichas de datos dentro de paneles** sin caja anidada: reglas internas en
  lugar de un segundo marco, y el último campo impar ocupa la fila completa.
- **Pestañas**: `.tabs` en Control Center (cambio de conjunto de datos) y
  `.section-tabs` en el Portal (secciones de una organización). El estado activo
  se expresa con `aria-current`/`is-active`, nunca con el modificador de
  paginación.
- **Formularios**: etiquetas alineadas arriba aunque un campo sea un textarea; el
  botón de envío se ancla al pie de la fila. Controles nativos (checkbox, radio)
  mantienen su tamaño y no heredan el estilo de los campos de texto.
- **Botones**: `.button` trae las métricas de tamaño por defecto, de modo que un
  enlace con la clase base nunca se colapse; los modificadores solo ajustan
  densidad.
- **Tablas**: acciones de fila compactas, densidad reducida en pantallas
  estrechas y scroll contenido en lugar de comprimir columnas.
- **Estados**: un fallo de API muestra el banner de error y **no** además el
  estado vacío correspondiente.

## Responsive y accesibilidad

Breakpoints validados: 1920×1080, 1440×900, 1366×768, 1024×768, 768×1024,
390×844 y 320×720.

- Sin desbordamiento horizontal accidental; tablas con scroll contenido y modo
  tarjeta donde el componente lo define.
- En el shell móvil, las filas del grid son explícitas
  (`auto` + `minmax(0, 1fr)`): sin ellas el alto sobrante se reparte entre la
  barra y el contenido y el topbar aparece a media página en vistas cortas.
- La navegación horizontal (barra del Control Center y pestañas de sección del
  Portal) indica continuidad con un degradado en el borde derecho.
- Foco visible con `#58DCF5`, navegación por teclado y skip link en el shell.
- Contraste AA en texto e indicadores; `prefers-reduced-motion` desactiva
  transiciones.
- La información importante no se oculta en móvil.

## Evidencia visual

Capturas bajo demanda (no comparan píxeles). Los specs se omiten sin
`HIQNET_VISUAL_DIR`:

```bash
HIQNET_VISUAL_DIR=/tmp/hiqnet-visual pnpm --filter @hiqnet/control-center e2e visual

# Portal: el config visual añade el servidor de desarrollo
cd apps/portal
HIQNET_VISUAL_DIR=visual-artifacts pnpm exec playwright test visual \
  --config=playwright.visual.config.ts
```

Guardas que sí se ejecutan siempre (API simulada, sin backend):

- `apps/control-center/e2e/layout.spec.ts`: el topbar arranca donde termina la
  barra móvil y no hay desbordamiento horizontal a 320 px.
- `apps/control-center/e2e/visual-contrast.spec.ts` y
  `apps/portal/e2e/visual-contrast.spec.ts`: contraste AA calculado sobre colores
  computados de roles clave (títulos, textos, botones, badges, navegación).

## Reglas para futuras pantallas

1. Reutilizar tokens y componentes existentes antes de crear variantes.
2. El acento es único; el degradado de marca no se aplica a superficies.
3. Evitar glassmorphism, glow permanente, sombras grandes y animaciones
   decorativas.
4. Toda pantalla nueva debe cubrir los estados y los breakpoints anteriores.
5. No introducir una librería UI adicional sin evaluar primero el valor real
   frente a la complejidad que añade.
6. Mantener la jerarquía: un `h1` por pantalla; el resto de encabezados baja de
   nivel, no de estilo.
7. Control Center y Portal comparten marca y criterios, pero no son copias:
   Control Center prioriza densidad operativa; el Portal, lectura y acciones del
   cliente.
