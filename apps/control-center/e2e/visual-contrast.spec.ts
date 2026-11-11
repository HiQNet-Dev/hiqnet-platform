import { expect, test } from "@playwright/test";

// Verificación de contraste WCAG AA sobre el tema de marca. No depende de
// capturas: lee colores computados y calcula la relación de contraste.
const outDir = process.env.HIQNET_VISUAL_DIR;

const staff = {
  account_id: "00000000-0000-4000-8000-000000000001",
  display_name: "Control Admin",
  email: "admin@hiqnet.test",
  role: "platform_admin",
  capabilities: ["products.read", "products.manage"],
};

const product = {
  id: "00000000-0000-4000-8000-000000000010",
  key: "pos",
  name: "HiQNet POS",
  description: "Punto de venta local-first.",
  status: "active",
  product_type: "desktop",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-02-10T00:00:00Z",
};

test("contraste AA de textos e indicadores clave", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(staff) }),
  );
  await page.route("**/api/v1/control/products**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [product], next_cursor: null }),
    }),
  );

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/control/products");
  await page.waitForLoadState("networkidle");

  const results = await page.evaluate(() => {
    const parse = (value: string) => {
      const match = value.match(/rgba?\(([^)]+)\)/);
      if (!match) return null;
      const [r, g, b, a = "1"] = match[1].split(",").map((part) => part.trim());
      return { r: Number(r), g: Number(g), b: Number(b), a: Number(a) };
    };
    const luminance = (color: { r: number; g: number; b: number }) => {
      const channel = (value: number) => {
        const c = value / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
    };
    // Compone color de texto sobre el fondo efectivo (sin transparencias).
    const effective = (element: Element): { fg: ReturnType<typeof parse>; bg: ReturnType<typeof parse> } => {
      let background: ReturnType<typeof parse> = null;
      let node: Element | null = element;
      while (node) {
        const candidate = parse(getComputedStyle(node).backgroundColor);
        if (candidate && candidate.a > 0.6) {
          background = candidate;
          break;
        }
        node = node.parentElement;
      }
      return {
        fg: parse(getComputedStyle(element).color),
        bg: background ?? { r: 13, g: 15, b: 18, a: 1 },
      };
    };
    const ratio = (fg: ReturnType<typeof parse>, bg: ReturnType<typeof parse>) => {
      if (!fg || !bg) return 0;
      const l1 = luminance(fg);
      const l2 = luminance(bg);
      const [light, dark] = l1 > l2 ? [l1, l2] : [l2, l1];
      return (light + 0.05) / (dark + 0.05);
    };
    const selectors = [
      ".page h1",
      ".page__heading p",
      ".nav-link",
      ".nav-link.is-active",
      ".breadcrumbs",
      ".table-link code",
      ".badge--active",
      ".button--primary",
      ".eyebrow",
    ];
    return selectors.flatMap((selector) => {
      const element = document.querySelector(selector);
      if (!element) return [];
      const { fg, bg } = effective(element);
      return [{ selector, ratio: Number(ratio(fg, bg).toFixed(2)) }];
    });
  });

  for (const { selector, ratio } of results) {
    expect(ratio, `contraste insuficiente en ${selector} (${ratio})`).toBeGreaterThanOrEqual(4.5);
  }

  if (outDir) {
    await page.screenshot({ path: `${outDir}/contrast-page.png`, fullPage: true });
  }
});
