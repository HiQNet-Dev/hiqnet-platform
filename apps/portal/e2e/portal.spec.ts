import { expect, test } from '@playwright/test';

const email = process.env.PORTAL_E2E_EMAIL;
const password = process.env.PORTAL_E2E_PASSWORD;

test.skip(!email || !password, 'Define PORTAL_E2E_EMAIL y PORTAL_E2E_PASSWORD');

test('login, organización, miembros y seguridad de cuenta', async ({ page }) => {
  await page.goto('/login');
  await page.getByLabel('Correo electrónico').fill(email ?? '');
  await page.getByLabel('Contraseña').fill(password ?? '');
  await page.getByRole('button', { name: 'Entrar' }).click();

  await expect(page.getByRole('heading', { name: 'Organizaciones', exact: true })).toBeVisible();

  const organizationName = `Portal E2E ${Date.now()}`;
  await page.getByRole('button', { name: 'Nueva organización' }).click();
  const createDialog = page.getByRole('dialog');
  await createDialog.getByLabel('Nombre de la organización').fill(organizationName);
  await createDialog.getByRole('button', { name: 'Crear organización' }).click();

  await expect(page.getByRole('heading', { name: organizationName })).toBeVisible();
  await page.getByRole('link', { name: 'Miembros', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Miembros' })).toBeVisible();
  await expect(page.getByText('Propietario').first()).toBeVisible();

  await page.getByRole('link', { name: 'Cuenta y seguridad' }).click();
  await expect(page.getByRole('heading', { name: 'Cuenta y seguridad' })).toBeVisible();

  await page.getByRole('button', { name: 'Cerrar esta sesión' }).click();
  await expect(page).toHaveURL(/\/login/);
});
