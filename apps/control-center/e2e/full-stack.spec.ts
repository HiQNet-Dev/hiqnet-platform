import { expect, test } from "@playwright/test";

import {
  activateOrganizationProduct,
  createPlan,
  createProduct,
  createSubscription,
  getEntitlements,
  getOrganization,
  SEEDED_ORGANIZATION_ID,
  staffLogin,
} from "./helpers/commercial";

/**
 * Recorrido comercial completo por la interfaz del Control Center contra la API
 * Axum y PostgreSQL reales. Se omite cuando no hay credenciales staff, para que
 * la suite con API simulada siga siendo ejecutable en solitario.
 *
 * Reutiliza la organización sembrada por el bootstrap E2E en lugar de crear una
 * por viewport: la creación de organizaciones está limitada a 5 por minuto.
 */
const staffEmail = process.env.PLATFORM_STAFF_EMAIL;
const staffPassword = process.env.PLATFORM_STAFF_PASSWORD;

test.skip(
  !staffEmail || !staffPassword,
  "Define PLATFORM_STAFF_EMAIL y PLATFORM_STAFF_PASSWORD para el E2E full-stack",
);

test("producto no-POS: asociación, plan, suscripción y entitlements desde la UI", async ({
  page,
}) => {
  const staffCookie = await staffLogin(staffEmail ?? "", staffPassword ?? "");
  const organization = await getOrganization(staffCookie, SEEDED_ORGANIZATION_ID);
  const product = await createProduct(staffCookie);
  await activateOrganizationProduct(staffCookie, organization.id, product.id);
  const plan = await createPlan(staffCookie, product.id);
  const subscription = await createSubscription(
    staffCookie,
    organization.id,
    product.id,
    plan.id,
  );

  // La UI staff se autentica contra la API real (cookie hiqnet_staff_session).
  await page.goto("/control/login");
  await page.getByLabel("Email").fill(staffEmail ?? "");
  await page.getByLabel("Contraseña").fill(staffPassword ?? "");
  await page.getByRole("button", { name: "Iniciar sesión" }).click();
  await expect(page.getByRole("heading", { name: "Products", exact: true })).toBeVisible();

  // El login por UI rota la sesión previa: la cookie válida es la del navegador.
  const uiSession = (await page.context().cookies()).find(
    (cookie) => cookie.name === "hiqnet_staff_session",
  );
  if (!uiSession) throw new Error("El Control Center no estableció la cookie staff");
  const uiCookie = `hiqnet_staff_session=${uiSession.value}`;

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

  // El detalle de la organización refleja la pérdida de derechos.
  await page.goto(`/control/organizations/${organization.id}`);
  await expect(page.getByText("suspended", { exact: true }).first()).toBeVisible();
});
