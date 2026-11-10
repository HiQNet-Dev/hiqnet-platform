import { expect, test } from "@playwright/test";

import {
  activateOrganizationProduct,
  createPlan,
  createProduct,
  createProductStatus,
  createSubscription,
  findAuditEvent,
  getEntitlements,
  getOrganization,
  getProduct,
  SEEDED_ORGANIZATION_ID,
  staffLogin,
} from "./helpers/api";

/**
 * Recorrido completo por la interfaz del Control Center contra la API Axum y
 * PostgreSQL reales. Se omite cuando faltan credenciales staff, para que la
 * suite con API simulada siga siendo ejecutable en solitario.
 *
 * Reutiliza la organización sembrada por el bootstrap E2E en lugar de crear una
 * por viewport: la creación de organizaciones está limitada a 5 por minuto.
 */
const staffEmail = process.env.PLATFORM_STAFF_EMAIL;
const staffPassword = process.env.PLATFORM_STAFF_PASSWORD;
const readonlyEmail = process.env.PLATFORM_READONLY_EMAIL;
const readonlyPassword = process.env.PLATFORM_READONLY_PASSWORD;

test.skip(
  !staffEmail || !staffPassword,
  "Define PLATFORM_STAFF_EMAIL y PLATFORM_STAFF_PASSWORD para el E2E full-stack",
);

async function login(page: import("@playwright/test").Page) {
  await page.goto("/control/login");
  await page.getByLabel("Email").fill(staffEmail ?? "");
  await page.getByLabel("Contraseña").fill(staffPassword ?? "");
  await page.getByRole("button", { name: "Iniciar sesión" }).click();
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();
  // El login por UI rota la sesión previa: la cookie válida es la del navegador.
  const session = (await page.context().cookies()).find(
    (cookie) => cookie.name === "hiqnet_staff_session",
  );
  if (!session) throw new Error("El Control Center no estableció la cookie staff");
  return `hiqnet_staff_session=${session.value}`;
}

test("producto no-POS: plan, suscripción y entitlements desde la UI", async ({ page }) => {
  const setupCookie = await staffLogin(staffEmail ?? "", staffPassword ?? "");
  const organization = await getOrganization(setupCookie, SEEDED_ORGANIZATION_ID);
  const product = await createProduct(setupCookie);
  await activateOrganizationProduct(setupCookie, organization.id, product.id);
  const plan = await createPlan(setupCookie, product.id);
  const subscription = await createSubscription(
    setupCookie,
    organization.id,
    product.id,
    plan.id,
  );

  const uiCookie = await login(page);

  await page.goto(`/control/products/${product.key}`);
  await expect(page.getByRole("heading", { name: product.name })).toBeVisible();

  await page.goto(`/control/organizations/${organization.id}`);
  await expect(
    page.getByRole("heading", { name: organization.display_name, exact: true }),
  ).toBeVisible();
  await expect(page.getByText(product.name)).toBeVisible();
  await expect(page.getByText("effective", { exact: true }).first()).toBeVisible();

  await page.goto(`/control/plans/${plan.id}`);
  await expect(page.getByRole("heading", { name: plan.name })).toBeVisible();

  // Mutación desde el navegador: exige Origin del Control Center y RBAC staff.
  await page.goto(`/control/subscriptions/${subscription.id}`);
  await page.getByRole("button", { name: "Suspend", exact: true }).click();
  await expect(page.getByText("suspended", { exact: true }).first()).toBeVisible();

  const suspended = await getEntitlements(uiCookie, organization.id, product.id);
  expect(suspended.effective).toBe(false);
  expect(suspended.reason).toBe("suspended");

  await page.goto(`/control/organizations/${organization.id}`);
  await expect(page.getByText("suspended", { exact: true }).first()).toBeVisible();
});

test("un administrador crea y edita un producto desde la interfaz", async ({ page }) => {
  const uiCookie = await login(page);
  const key = `ui-${crypto.randomUUID().slice(0, 8)}`;
  const originalName = `Producto UI ${key}`;
  const editedName = `${originalName} editado`;

  await page.goto("/control/products");
  const createForm = page.getByRole("form", { name: "Crear producto" });
  await createForm.getByLabel("Key").fill(key);
  await createForm.getByLabel("Name").fill(originalName);
  await createForm.getByLabel("Type").selectOption("saas");
  await createForm.getByLabel("Status").selectOption("active");
  await createForm.getByLabel("Description").fill("Creado desde el Control Center");
  await createForm.getByRole("button", { name: "Create product" }).click();

  await expect(page).toHaveURL(new RegExp(`/control/products/${key}$`));
  await expect(page.getByRole("heading", { name: originalName })).toBeVisible();

  const created = await getProduct(uiCookie, key);
  expect(created.status).toBe("active");

  // Edición de campos permitidos: sin confirmación, sólo los campos cambiados.
  const editor = page.getByRole("region", { name: "Editar producto" });
  await editor.getByLabel("Name").fill(editedName);
  await editor.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByRole("heading", { name: editedName })).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Cambio de estado: confirma antes de mutar.
  await editor.getByLabel("Status").selectOption("maintenance");
  await editor.getByRole("button", { name: "Save changes" }).click();
  const dialog = page.getByRole("dialog", { name: "Confirm status change" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("maintenance");
  await dialog.getByRole("button", { name: "Confirm change" }).click();
  await expect(page.getByText("maintenance", { exact: true }).first()).toBeVisible();

  const updated = await getProduct(uiCookie, key);
  expect(updated.name).toBe(editedName);
  expect(updated.status).toBe("maintenance");

  // Integración con auditoría: las mutaciones dejan eventos consultables.
  expect(await findAuditEvent(uiCookie, "product_created", created.id)).toBeTruthy();
  expect(await findAuditEvent(uiCookie, "product_updated", created.id)).toBeTruthy();
});

test("un rol read_only no puede crear productos ni por API", async () => {
  test.skip(
    !readonlyEmail || !readonlyPassword,
    "Define PLATFORM_READONLY_EMAIL y PLATFORM_READONLY_PASSWORD para verificar RBAC directo",
  );
  const cookie = await staffLogin(readonlyEmail ?? "", readonlyPassword ?? "");
  const key = `forbidden-${crypto.randomUUID().slice(0, 8)}`;
  const status = await createProductStatus(cookie, key);
  expect(status).toBe(403);
  await expect
    .poll(async () => {
      try {
        await getProduct(cookie, key);
        return 200;
      } catch {
        return 404;
      }
    })
    .toBe(404);
});
