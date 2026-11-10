import { expect, test } from '@playwright/test';

import {
  activateOrganizationProduct,
  commercialLogin,
  createCommercialOrganization,
  createPlan,
  createProduct,
  createSubscription,
  expectOtherTenantNotFound,
  getEntitlements,
  getPortalProduct,
  staffLogin,
  transitionSubscription,
} from './helpers/commercial';

const email = process.env.PORTAL_E2E_EMAIL;
const password = process.env.PORTAL_E2E_PASSWORD;
const staffEmail = process.env.PLATFORM_STAFF_EMAIL;
const staffPassword = process.env.PLATFORM_STAFF_PASSWORD;

test.skip(
  !email || !password || !staffEmail || !staffPassword,
  'Define credenciales comerciales y staff para el E2E full-stack',
);

test('producto sintético no-POS: plan, suscripción, entitlements y aislamiento', async ({
  page,
}) => {
  const staffCookie = await staffLogin(staffEmail ?? '', staffPassword ?? '');
  const commercialCookie = await commercialLogin(email ?? '', password ?? '');
  const organization = await createCommercialOrganization(commercialCookie);
  const product = await createProduct(staffCookie);
  await activateOrganizationProduct(staffCookie, organization.id, product.id);
  const plan = await createPlan(staffCookie, product.id);
  const subscription = await createSubscription(staffCookie, organization.id, product.id, plan.id);

  const effective = await getEntitlements(staffCookie, organization.id, product.id);
  expect(effective.effective).toBe(true);
  expect(effective.reason).toBe('active');

  await page.goto('/login');
  await page.getByLabel('Correo electrónico').fill(email ?? '');
  await page.getByLabel('Contraseña').fill(password ?? '');
  await page.getByRole('button', { name: 'Entrar' }).click();
  await expect(page.getByRole('heading', { name: 'Organizaciones', exact: true })).toBeVisible();
  const sessionCookie = (await page.context().cookies()).find(
    (cookie) => cookie.name === 'hiqnet_session',
  );
  if (!sessionCookie) throw new Error('El Portal no estableció la cookie comercial');
  const uiCookie = `hiqnet_session=${sessionCookie.value}`;
  await page.getByRole('link', { name: organization.display_name, exact: true }).click();
  await page.getByRole('link', { name: 'Productos', exact: true }).click();
  await expect(page.getByText(product.name)).toBeVisible();
  await expect(page.getByText('efectivos', { exact: true })).toBeVisible();

  await transitionSubscription(staffCookie, subscription.id, 'suspend');
  await page.reload();
  await expect(page.getByText('suspended', { exact: true }).first()).toBeVisible();
  const suspended = await getPortalProduct(uiCookie, organization.id, product.id);
  expect(suspended.entitlements.effective).toBe(false);

  await transitionSubscription(staffCookie, subscription.id, 'reactivate');
  await page.reload();
  await expect(page.getByText('efectivos', { exact: true })).toBeVisible();

  await transitionSubscription(staffCookie, subscription.id, 'cancel');
  await page.reload();
  await expect(page.getByText('terminated', { exact: true })).toBeVisible();

  await expectOtherTenantNotFound(uiCookie);
});
