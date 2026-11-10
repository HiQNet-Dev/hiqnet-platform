import { expect, test } from "@playwright/test";

const staff = { account_id: "00000000-0000-4000-8000-000000000001", display_name: "Control Admin", email: "admin@hiqnet.test", role: "platform_admin", capabilities: ["products.read", "products.manage"] };
const product = { id: "00000000-0000-4000-8000-000000000010", key: "pos", name: "HiQNet POS", description: "Punto de venta", status: "active", product_type: "desktop", created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-02T00:00:00Z" };
const organization = { id: "00000000-0000-4000-8000-000000000020", display_name: "Org Alpha", status: "active", created_at: "2026-01-03T00:00:00Z" };
const archivedOrganization = { id: "00000000-0000-4000-8000-000000000021", display_name: "Org Archived", status: "archived", created_at: "2026-01-04T00:00:00Z" };
const organizationDetail = {
  ...organization,
  memberships: [
    { account_id: "00000000-0000-4000-8000-000000000030", email: "owner@example.test", display_name: "Owner Account", account_status: "active", role: "owner", joined_at: "2026-01-03T00:00:00Z", revoked_at: null },
    { account_id: "00000000-0000-4000-8000-000000000031", email: "revoked@example.test", display_name: "Revoked Account", account_status: "disabled", role: "member", joined_at: "2026-01-04T00:00:00Z", revoked_at: "2026-01-05T00:00:00Z" },
  ],
  products: [
    { product_id: "00000000-0000-4000-8000-000000000010", product_key: "pos", product_name: "HiQNet POS", product_status: "active", association_status: "active", activated_at: "2026-01-03T00:00:00Z", entitlements: {} },
    { product_id: "00000000-0000-4000-8000-000000000011", product_key: "legacy", product_name: "Legacy Suite", product_status: "retired", association_status: "inactive", activated_at: "2026-01-04T00:00:00Z", entitlements: {} },
  ],
};
const account = { id: "00000000-0000-4000-8000-000000000030", email: "owner@example.test", display_name: "Owner Account", status: "active", created_at: "2026-01-03T00:00:00Z" };
const accountDetail = {
  ...account,
  memberships: [
    { organization_id: organization.id, organization_display_name: organization.display_name, organization_status: "active", role: "owner", joined_at: "2026-01-03T00:00:00Z", revoked_at: "2026-01-06T00:00:00Z" },
  ],
};
const fullCapabilities = ["products.read", "products.manage", "organizations.read", "accounts.read"];


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
    if (url.pathname.endsWith("/auth/me")) return route.fulfill({ status: authenticated ? 200 : 401, contentType: "application/json", body: authenticated ? JSON.stringify({ ...staff, capabilities: fullCapabilities }) : "{}" });
    if (url.pathname.endsWith("/auth/logout")) { authenticated = false; return route.fulfill({ status: 204 }); }
    if (url.pathname.endsWith("/products")) return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [product], next_cursor: null }) });
    if (url.pathname.endsWith("/products/pos")) return route.fulfill({ contentType: "application/json", body: JSON.stringify(product) });
    if (url.pathname.endsWith("/projects")) return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [{ ...product, key: "pos-desktop", name: "POS Desktop", project_type: "application", product_key: "pos" }], next_cursor: null }) });
    if (url.pathname.endsWith("/organizations")) return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [], next_cursor: null }) });
    return route.fulfill({ status: 404, body: "{}" });
  });
  await page.goto("/control/products");
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();
  await page.getByRole("link", { name: /HiQNet POS/ }).click();
  await expect(page.getByRole("heading", { name: "HiQNet POS" })).toBeVisible();
  await expect(page.getByText("POS Desktop")).toBeVisible();
  await page.getByRole("link", { name: "Organizations" }).click();
  await expect(page.getByRole("heading", { name: "Organizations", exact: true })).toBeVisible();
  await expect(page.getByText("Módulo disponible en C4")).toHaveCount(0);
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

test("organizations reales: filtro, búsqueda, cursor y detalle con historial y productos", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/organizations**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname !== "/api/v1/control/organizations") return route.fulfill({ contentType: "application/json", body: JSON.stringify(organizationDetail) });
    const items = url.searchParams.get("status") === "archived" ? [archivedOrganization] : [organization];
    return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items, next_cursor: null }) });
  });
  await page.goto("/control/organizations");
  await expect(page.getByText("Org Alpha")).toBeVisible();
  await page.getByLabel("Status").selectOption("archived");
  await expect(page.getByText("Org Archived")).toBeVisible();
  await page.getByLabel("Status").selectOption("");
  await page.getByLabel("Search").fill("Alpha");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page).toHaveURL(/search=Alpha/);
  await page.getByRole("link", { name: /Org Alpha/ }).click();
  await expect(page.getByRole("heading", { name: "Org Alpha", exact: true })).toBeVisible();
  await expect(page.getByText("Revoked Account")).toBeVisible();
  await expect(page.getByText("2026", { exact: false }).first()).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Lifecycle" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Association" })).toBeVisible();
  await expect(page.getByText("Legacy Suite")).toBeVisible();
  await expect(page.getByText("inactive", { exact: true })).toBeVisible();
});

test("cambiar filtro elimina el cursor de organizations", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/organizations**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [organization], next_cursor: null }) }));
  await page.goto("/control/organizations?status=active&cursor=stale-cursor");
  await page.getByLabel("Status").selectOption("archived");
  await expect(page).toHaveURL(/status=archived/);
  await expect(page).not.toHaveURL(/cursor=/);
});

test("accounts reales: listado, filtros y detalle con membership revocada", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/accounts**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname !== "/api/v1/control/accounts") return route.fulfill({ contentType: "application/json", body: JSON.stringify(accountDetail) });
    const items = url.searchParams.get("status") === "disabled" ? [] : [account];
    return route.fulfill({ contentType: "application/json", body: JSON.stringify({ items, next_cursor: null }) });
  });
  await page.goto("/control/accounts");
  await expect(page.getByText("Owner Account")).toBeVisible();
  await page.getByLabel("Status").selectOption("disabled");
  await expect(page.getByText("No accounts found")).toBeVisible();
  await page.getByLabel("Status").selectOption("");
  await page.getByText("Owner Account").first().click();
  await expect(page.getByRole("heading", { name: "Owner Account", exact: true })).toBeVisible();
  await expect(page.getByText("Org Alpha")).toBeVisible();
  await expect(page.getByText("2026", { exact: false }).first()).toBeVisible();
});

test("sin capabilities no se muestran Organizations ni Accounts", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: ["products.read"] }) }));
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [], next_cursor: null }) }));
  await page.goto("/control/products");
  await expect(page.getByRole("link", { name: "Products" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Organizations" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Accounts" })).toHaveCount(0);
});

test("403 en Organizations y Accounts muestra acceso insuficiente", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/organizations**", (route) => route.fulfill({ status: 403, contentType: "application/json", body: JSON.stringify({ message: "forbidden" }) }));
  await page.route("**/api/v1/control/accounts**", (route) => route.fulfill({ status: 403, contentType: "application/json", body: JSON.stringify({ message: "forbidden" }) }));
  await page.goto("/control/organizations");
  await expect(page.getByRole("heading", { name: "Acceso insuficiente" })).toBeVisible();
  await page.goto("/control/accounts");
  await expect(page.getByRole("heading", { name: "Acceso insuficiente" })).toBeVisible();
});

test("401 durante una request C4 redirige a login", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/accounts**", (route) => route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ message: "session expired" }) }));
  await page.goto("/control/accounts");
  await expect(page.getByRole("heading", { name: "Control Center" })).toBeVisible();
});

test("C4 no consume endpoints comerciales", async ({ page }) => {
  await page.route("**/api/v1/organizations**", (route) => route.abort("failed"));
  await page.route("**/api/v1/accounts**", (route) => route.abort("failed"));
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }));
  await page.route("**/api/v1/control/organizations**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [organization], next_cursor: null }) }));
  await page.route("**/api/v1/control/accounts**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [account], next_cursor: null }) }));
  await page.goto("/control/organizations");
  await expect(page.getByText("Org Alpha")).toBeVisible();
  await page.getByRole("link", { name: "Accounts" }).click();
  await expect(page.getByText("Owner Account")).toBeVisible();
});

test("staff read-only puede consultar el registry sin acciones de gestión", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: ["products.read"] }) }));
  await page.route("**/api/v1/control/products**", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [product], next_cursor: null }) }));
  await page.goto("/control/products");
  await expect(page.getByText("HiQNet POS")).toBeVisible();
  await expect(page.getByRole("button", { name: "New product" })).toHaveCount(0);
});
