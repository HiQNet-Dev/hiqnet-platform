import { expect, test, type Page } from "@playwright/test";

/**
 * Guardas de composición del shell con API simulada: comprueban propiedades
 * de layout que no dependen de datos reales ni del backend. Se ejecutan
 * siempre, también en CI, porque no generan artefactos.
 */

const staff = {
  account_id: "00000000-0000-4000-8000-000000000001",
  display_name: "Control Admin",
  email: "admin@hiqnet.test",
  role: "platform_admin",
  capabilities: ["accounts.read"],
};

const account = {
  id: "22222222-2222-4222-8222-222222222222",
  email: "cliente@example.test",
  display_name: "Cliente Ejemplo",
  status: "active",
  created_at: "2026-01-02T00:00:00Z",
};

function json(body: unknown) {
  return { status: 200, contentType: "application/json", body: JSON.stringify(body) };
}

async function mockAccounts(page: Page) {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill(json(staff)));
  await page.route("**/api/v1/control/accounts**", (route) =>
    route.fulfill(json({ items: [account], next_cursor: null })),
  );
}

test("el topbar sigue a la barra móvil sin hueco en páginas cortas", async ({ page }) => {
  await page.setViewportSize({ width: 768, height: 1024 });
  await mockAccounts(page);
  await page.goto("/control/accounts");
  await expect(page.getByRole("heading", { level: 1, name: "Accounts" })).toBeVisible();

  const sidebar = await page.locator(".sidebar").boundingBox();
  const topbar = await page.locator(".topbar").boundingBox();
  expect(sidebar).not.toBeNull();
  expect(topbar).not.toBeNull();

  // El grid del shell no debe repartir el alto sobrante entre barra y contenido:
  // eso dejaba el topbar flotando a media página en vistas con poco contenido.
  const gap = topbar!.y - (sidebar!.y + sidebar!.height);
  expect(Math.abs(gap)).toBeLessThanOrEqual(1);
});

test("pantallas estrechas sin desbordamiento horizontal del documento", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 720 });
  await mockAccounts(page);
  await page.goto("/control/accounts");
  await expect(page.getByRole("heading", { level: 1, name: "Accounts" })).toBeVisible();

  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});
