import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';

import {
  beginSyntheticEnrollment,
  completeSyntheticProof,
  createSyntheticPos,
} from './helpers/synthetic-pos';

const email = process.env.PORTAL_E2E_EMAIL;
const password = process.env.PORTAL_E2E_PASSWORD;

test.skip(!email || !password, 'Define PORTAL_E2E_EMAIL y PORTAL_E2E_PASSWORD');

const organizationName = process.env.PORTAL_E2E_ORGANIZATION ?? 'Enrollment E2E Base';

async function loginAndOpenOrganization(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Correo electrónico').fill(email ?? '');
  await page.getByLabel('Contraseña').fill(password ?? '');
  await page.getByRole('button', { name: 'Entrar' }).click();
  await expect(page.getByRole('heading', { name: 'Organizaciones', exact: true })).toBeVisible();

  await page.getByRole('link', { name: organizationName, exact: true }).click();
  await expect(page.getByRole('heading', { name: organizationName, exact: true })).toBeVisible();
  const organizationId = page.url().split('/').pop();
  if (!organizationId) {
    throw new Error('No se pudo obtener el organization_id');
  }
  return { organizationName, organizationId };
}

test('POS sintético → approve en Portal → proof → instalación visible', async ({ page }) => {
  const { organizationName, organizationId } = await loginAndOpenOrganization(page);
  const pos = createSyntheticPos();
  const challenge = await beginSyntheticEnrollment(pos);

  await page.goto(`/enroll?code=${encodeURIComponent(challenge.user_code)}`);
  await expect(page.getByRole('heading', { name: 'Vincular una instalación' })).toBeVisible();
  await page.getByRole('button', { name: 'Continuar' }).click();
  await expect(page.getByRole('heading', { name: 'Confirmar vinculación' })).toBeVisible();
  await page.getByLabel('Organización propietaria').selectOption({ label: organizationName });
  await page.getByRole('button', { name: 'Aprobar instalación' }).click();
  await expect(page.getByRole('heading', { name: 'Instalación aprobada' })).toBeVisible();

  await page.goto(`/app/organizations/${organizationId}/installations`);
  await expect(page.getByRole('heading', { name: 'Instalaciones', exact: true })).toBeVisible();

  const completion = await completeSyntheticProof(pos, challenge);
  expect(completion.status).toBe('completed');

  await page.getByRole('button', { name: 'Actualizar' }).click();
  await expect(page.getByText(pos.deviceLabel)).toBeVisible();
});

test('código inexistente muestra un error neutral', async ({ page }) => {
  const { organizationName } = await loginAndOpenOrganization(page);
  await page.goto('/enroll?code=ZZZZ-ZZZZ');
  await page.getByRole('button', { name: 'Continuar' }).click();
  await page.getByLabel('Organización propietaria').selectOption({ label: organizationName });
  await page.getByRole('button', { name: 'Aprobar instalación' }).click();
  await expect(page.getByRole('heading', { name: 'Código no encontrado' })).toBeVisible();
});
