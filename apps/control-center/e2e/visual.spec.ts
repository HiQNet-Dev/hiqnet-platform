import { test } from "@playwright/test";

/**
 * Capturas de revisión visual bajo demanda.
 *
 * Se omiten salvo que `HIQNET_VISUAL_DIR` apunte a un directorio de salida:
 * `HIQNET_VISUAL_DIR=/tmp/hiqnet-visual pnpm --filter @hiqnet/control-center e2e visual`.
 *
 * No comparan píxeles: generan evidencia para revisar composición, jerarquía y
 * responsive en los breakpoints acordados sin añadir tiempo al CI obligatorio.
 */
const outDir = process.env.HIQNET_VISUAL_DIR;

const staff = {
  account_id: "00000000-0000-4000-8000-000000000001",
  display_name: "Control Admin",
  email: "admin@hiqnet.test",
  role: "platform_admin",
  capabilities: [
    "products.read",
    "products.manage",
    "organizations.read",
    "accounts.read",
    "installations.read",
    "subscriptions.read",
  ],
};

const products = [
  {
    id: "00000000-0000-4000-8000-000000000010",
    key: "pos",
    name: "HiQNet POS",
    description: "Punto de venta local-first.",
    status: "active",
    product_type: "desktop",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-02-10T00:00:00Z",
  },
  {
    id: "00000000-0000-4000-8000-000000000011",
    key: "platform",
    name: "HiQNet Platform",
    description: "Plano de control comercial.",
    status: "active",
    product_type: "saas",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-02-11T00:00:00Z",
  },
  {
    id: "00000000-0000-4000-8000-000000000012",
    key: "legacy-suite",
    name: "Legacy Suite",
    description: "Producto anterior en mantenimiento.",
    status: "maintenance",
    product_type: "integration",
    created_at: "2025-11-01T00:00:00Z",
    updated_at: "2026-01-20T00:00:00Z",
  },
];

const viewports = [
  [1920, 1080],
  [1440, 900],
  [1024, 768],
  [768, 1024],
  [390, 844],
  [320, 720],
] as const;

test("capturas de login y listado (bajo demanda)", async ({ page }) => {
  test.skip(!outDir, "define HIQNET_VISUAL_DIR para generar capturas");

  await page.route("**/api/v1/control/products**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: products, next_cursor: null }),
    }),
  );

  await page.route("**/api/v1/control/auth/me", (route) =>
    route.fulfill({ status: 401, contentType: "application/json", body: "{}" }),
  );
  for (const [width, height] of viewports) {
    await page.setViewportSize({ width, height });
    await page.goto("/control/login");
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: `${outDir}/login-${width}.png` });
  }

  await page.unroute("**/api/v1/control/auth/me");
  await page.route("**/api/v1/control/auth/me", (route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(staff) }),
  );
  for (const [width, height] of viewports) {
    await page.setViewportSize({ width, height });
    await page.goto("/control/products");
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: `${outDir}/products-${width}.png`, fullPage: true });
  }
});
