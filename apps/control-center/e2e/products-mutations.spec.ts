import { expect, test, type Page } from "@playwright/test";

const product = {
  id: "00000000-0000-4000-8000-000000000010",
  key: "pos",
  name: "HiQNet POS",
  description: "Punto de venta",
  status: "active",
  product_type: "desktop",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-02T00:00:00Z",
};

function staff(capabilities: string[]) {
  return {
    account_id: "00000000-0000-4000-8000-000000000001",
    display_name: "Staff",
    email: "staff@hiqnet.test",
    role: capabilities.includes("products.manage") ? "platform_admin" : "read_only",
    capabilities,
  };
}

/**
 * Simula la API del Control Center. Devuelve las solicitudes de mutación
 * recibidas para poder afirmar qué se envió exactamente.
 */
async function mockControlApi(page: Page, capabilities: string[]) {
  const mutations: { method: string; path: string; body: unknown }[] = [];
  let current = { ...product };

  await page.route("**/api/v1/control/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();

    if (path.endsWith("/auth/me")) {
      return route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(staff(capabilities)),
      });
    }
    if (path.endsWith("/projects")) {
      return route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({ items: [], next_cursor: null }),
      });
    }
    if (method === "POST" && path.endsWith("/products")) {
      const body = request.postDataJSON() as { key: string; name: string };
      mutations.push({ method, path, body });
      const created = {
        ...current,
        id: "00000000-0000-4000-8000-000000000011",
        key: body.key,
        name: body.name,
        status: "development",
        product_type: "saas",
        description: "",
      };
      current = created;
      return route.fulfill({ status: 201, contentType: "application/json", body: JSON.stringify(created) });
    }
    if (method === "PATCH" && path.includes("/products/")) {
      const body = request.postDataJSON();
      mutations.push({ method, path, body });
      current = { ...current, ...(body as Partial<typeof product>) };
      return route.fulfill({ contentType: "application/json", body: JSON.stringify(current) });
    }
    if (path.includes("/products/")) {
      return route.fulfill({ contentType: "application/json", body: JSON.stringify(current) });
    }
    return route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ items: [current], next_cursor: null }),
    });
  });

  return mutations;
}

const readCapabilities = ["products.read", "organizations.read", "accounts.read"];

test("read_only no ve el alta ni la edición de productos", async ({ page }) => {
  await mockControlApi(page, readCapabilities);
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();
  await expect(page.getByRole("form", { name: "Crear producto" })).toHaveCount(0);

  await page.getByRole("link", { name: /HiQNet POS/ }).click();
  await expect(page.getByRole("heading", { name: "HiQNet POS" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Editar producto" })).toHaveCount(0);
});

test("products.manage crea un producto validado y navega al detalle", async ({ page }) => {
  const mutations = await mockControlApi(page, [...readCapabilities, "products.manage"]);
  await page.goto("/control/products");

  const form = page.getByRole("form", { name: "Crear producto" });
  await form.getByLabel("Key").fill("demo-service");
  await form.getByLabel("Name").fill("Demo Service");
  await form.getByLabel("Type").selectOption("saas");
  await form.getByLabel("Status").selectOption("active");
  await form.getByLabel("Description").fill("Producto sintético no-POS");
  await form.getByRole("button", { name: "Create product" }).click();

  await expect(page).toHaveURL(/\/control\/products\/demo-service$/);
  await expect(page.getByRole("heading", { name: "Demo Service" })).toBeVisible();
  expect(mutations).toHaveLength(1);
  expect(mutations[0].method).toBe("POST");
  expect(mutations[0].body).toEqual({
    key: "demo-service",
    name: "Demo Service",
    description: "Producto sintético no-POS",
    status: "active",
    type: "saas",
  });
});

test("el alta rechaza claves no normalizadas sin llamar a la API", async ({ page }) => {
  const mutations = await mockControlApi(page, [...readCapabilities, "products.manage"]);
  await page.goto("/control/products");

  const form = page.getByRole("form", { name: "Crear producto" });
  await form.getByLabel("Key").fill("Demo Service");
  await form.getByLabel("Name").fill("Demo");
  await form.getByRole("button", { name: "Create product" }).click();

  await expect(form.getByRole("alert")).toContainText("minúsculas, dígitos y guiones");
  expect(mutations).toHaveLength(0);
});

test("una clave duplicada muestra un mensaje neutral y no navega", async ({ page }) => {
  await page.route("**/api/v1/control/**", (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.pathname.endsWith("/auth/me")) {
      return route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(staff([...readCapabilities, "products.manage"])),
      });
    }
    if (request.method() === "POST" && url.pathname.endsWith("/products")) {
      return route.fulfill({
        status: 409,
        contentType: "application/json",
        body: JSON.stringify({ code: "conflict", message: "la clave ya existe" }),
      });
    }
    return route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ items: [product], next_cursor: null }),
    });
  });

  await page.goto("/control/products");
  const form = page.getByRole("form", { name: "Crear producto" });
  await form.getByLabel("Key").fill("pos");
  await form.getByLabel("Name").fill("Duplicado");
  await form.getByRole("button", { name: "Create product" }).click();

  await expect(form.getByRole("alert")).toContainText("Ya existe un producto con esa clave");
  await expect(page).toHaveURL(/\/control\/products$/);
});

test("la edición envía sólo los campos cambiados y confirma cambios de estado", async ({ page }) => {
  const mutations = await mockControlApi(page, [...readCapabilities, "products.manage"]);
  await page.goto("/control/products/pos");

  const region = page.getByRole("region", { name: "Editar producto" });
  await region.getByLabel("Name").fill("HiQNet POS 2");
  await region.getByRole("button", { name: "Save changes" }).click();

  await expect.poll(() => mutations.length).toBe(1);
  expect(mutations[0].body).toEqual({ name: "HiQNet POS 2" });
  expect(page.getByRole("dialog")).toHaveCount(0);

  await region.getByLabel("Status").selectOption("retired");
  await region.getByRole("button", { name: "Save changes" }).click();

  const dialog = page.getByRole("dialog", { name: "Confirm status change" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("retired");
  expect(mutations).toHaveLength(1);

  await dialog.getByRole("button", { name: "Confirm change" }).click();
  await expect.poll(() => mutations.length).toBe(2);
  expect(mutations[1].body).toEqual({ status: "retired" });
});
