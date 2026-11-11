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

## Responsive y accesibilidad

Breakpoints validados: 1920×1080, 1440×900, 1366×768, 1024×768, 768×1024,
390×844 y 320×720.

- Sin desbordamiento horizontal accidental; tablas con scroll contenido y modo
  tarjeta donde el componente lo define.
- Foco visible con `#58DCF5`, navegación por teclado y skip link en el shell.
- Contraste AA en texto e indicadores; `prefers-reduced-motion` desactiva
  transiciones.
- La información importante no se oculta en móvil.

## Evidencia visual

Capturas bajo demanda (no forman parte del CI obligatorio y no comparan píxeles):

```bash
HIQNET_VISUAL_DIR=/tmp/hiqnet-visual pnpm --filter @hiqnet/control-center e2e visual
HIQNET_VISUAL_DIR=/tmp/hiqnet-visual pnpm --filter @hiqnet/platform-portal e2e visual
```

Los specs viven en `apps/*/e2e/visual.spec.ts` y se omiten sin la variable de
entorno.

## Reglas para futuras pantallas

1. Reutilizar tokens y componentes existentes antes de crear variantes.
2. El acento es único; el degradado de marca no se aplica a superficies.
3. Evitar glassmorphism, glow permanente, sombras grandes y animaciones
   decorativas.
4. Toda pantalla nueva debe cubrir los estados y los breakpoints anteriores.
5. No introducir una librería UI adicional sin evaluar primero el valor real
   frente a la complejidad que añade.
