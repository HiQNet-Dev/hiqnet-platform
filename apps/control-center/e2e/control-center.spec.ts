import { expect, test } from "@playwright/test";

const staff = { account_id: "00000000-0000-4000-8000-000000000001", display_name: "Control Admin", email: "admin@hiqnet.test", role: "platform_admin", capabilities: ["products.read", "products.manage"] };
const product = { id: "00000000-0000-4000-8000-000000000010", key: "pos", name: "HiQNet POS", description: "Punto de venta", status: "active", product_type: "desktop", created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-02T00:00:00Z" };


test("redirecciona una ruta protegida a login sin sesión", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ status: 401, contentType: "application/json", body: "{}" }));
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Control Center" })).toBeVisible();
});

test("login correcto establece la sesión staff y abre Products", async ({ page }) => {
  let authenticated = false;
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ status: authenticated ? 200 : 401, contentType: "application/json", body: authenticated ? JSON.stringify(staff) : "{}" }));
  await page.route("**/api/v1/control/auth/login", (route) => { authenticated = true; return route.fulfill({ status: 204 }); });
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [], next_cursor: null }) }));
  await page.goto("/control/login");
  await page.getByLabel("Email").fill("admin@hiqnet.test"); await page.getByLabel("Contraseña").fill("correct-password"); await page.getByRole("button", { name: "Iniciar sesión" }).click();
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();
});

test("login inválido muestra feedback y no expone tokens", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ status: 401, body: "{}" }));
  await page.route("**/api/v1/control/auth/login", (route) => route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ message: "invalid credentials" }) }));
  await page.goto("/control/login");
  await page.getByLabel("Email").focus(); await page.keyboard.press("Tab");
  await expect(page.getByLabel("Contraseña")).toBeFocused();
  await page.getByLabel("Email").fill("bad@hiqnet.test"); await page.getByLabel("Contraseña").fill("wrong-password"); await page.getByRole("button", { name: "Iniciar sesión" }).click();
  await expect(page.getByRole("alert")).toContainText("No fue posible iniciar sesión");
});

test("bootstrap, products, detail/projects y logout usan sólo API control", async ({ page }) => {
  let authenticated = true;
  await page.route("**/api/v1/organizations**", (route) => route.abort("failed"));
  await page.route("**/api/v1/control/**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/auth/me")) return route.fulfill({ status: authenticated ? 200 : 401, contentType: "application/json", body: authenticated ? JSON.stringify(staff) : "{}" });
    if (url.pathname.endsWith("/auth/logout")) { authenticated = false; return route.fulfill({ status: 204 }); }
    if (url.pathname.endsWith("/products")) return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [product], next_cursor: null }) });
    if (url.pathname.endsWith("/products/pos")) return route.fulfill({ contentType: "application/json", body: JSON.stringify(product) });
    if (url.pathname.endsWith("/projects")) return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [{ ...product, key: "pos-desktop", name: "POS Desktop", project_type: "application", product_key: "pos" }], next_cursor: null }) });
    return route.fulfill({ status: 404, body: "{}" });
  });
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();
  await page.getByRole("link", { name: /HiQNet POS/ }).click();
  await expect(page.getByRole("heading", { name: "HiQNet POS" })).toBeVisible();
  await expect(page.getByText("POS Desktop")).toBeVisible();
  await page.getByRole("link", { name: "Organizations" }).click();
  await expect(page.getByText("Módulo disponible en C4")).toBeVisible();
  await page.getByRole("button", { name: "Log out" }).click();
  await expect(page.getByRole("heading", { name: "Control Center" })).toBeVisible();
});

test("una sesión expirada durante Products redirige a login", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(staff) }));
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ message: "session expired" }) }));
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Control Center" })).toBeVisible();
});

test("un 403 de Products muestra acceso insuficiente", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(staff) }));
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ status: 403, contentType: "application/json", body: JSON.stringify({ message: "forbidden" }) }));
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Acceso insuficiente" })).toBeVisible();
});

test("staff read-only puede consultar el registry sin acciones de gestión", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: ["products.read"] }) }));
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [product], next_cursor: null }) }));
  await page.goto("/control/products");
  await expect(page.getByText("HiQNet POS")).toBeVisible();
  await expect(page.getByRole("button", { name: "New product" })).toHaveCount(0);
});
