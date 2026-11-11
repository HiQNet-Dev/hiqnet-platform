import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';

const outDir = process.env.HIQNET_VISUAL_DIR;

const account = {
  account_id: '22222222-2222-4222-8222-222222222222',
  id: '22222222-2222-4222-8222-222222222222',
  email: 'cliente@example.test',
  display_name: 'Cliente Ejemplo',
};
const organizationId = '11111111-1111-4111-8111-111111111111';
const organization = {
  id: organizationId,
  display_name: 'Cafetería Central',
  status: 'active',
  created_at: '2026-01-03T00:00:00Z',
  memberships: [
    {
      account_id: account.account_id,
      email: account.email,
      display_name: account.display_name,
      account_status: 'active',
      role: 'owner',
      joined_at: '2026-01-03T00:00:00Z',
      revoked_at: null,
    },
  ],
  products: [],
};
const installation = {
  installation_id: '33333333-3333-4333-8333-333333333331',
  device_label: 'Caja mostrador',
  status: 'active',
  identity_status: 'active',
  bound_at: '2026-01-10T10:05:00Z',
  last_seen_at: '2026-02-12T09:30:00Z',
};

function json(body: unknown, status = 200) {
  return { status, contentType: 'application/json', body: JSON.stringify(body) };
}

async function installMocks(page: Page) {
  await page.route('**/api/v1/auth/me', (route) => route.fulfill(json(account)));
  await page.route('**/api/v1/organizations**', (route) =>
    route.fulfill(json({ items: [organization], next_cursor: null })),
  );
  await page.route(`**/api/v1/organizations/${organizationId}/members**`, (route) =>
    route.fulfill(json({ items: organization.memberships, next_cursor: null })),
  );
  await page.route(`**/api/v1/organizations/${organizationId}/installations**`, (route) =>
    route.fulfill(json({ items: [installation], next_cursor: null })),
  );
  await page.route(`**/api/v1/organizations/${organizationId}/products**`, (route) =>
    route.fulfill(json({ items: [], next_cursor: null })),
  );
  await page.route(`**/api/v1/organizations/${organizationId}`, (route) =>
    route.fulfill(json(organization)),
  );
}

test('smoke visual de todas las rutas del Portal', async ({ page }) => {
  test.skip(!outDir, 'define HIQNET_VISUAL_DIR para generar capturas');
  await installMocks(page);

  const routes = [
    ['/app/organizations', 'organizations'],
    [`/app/organizations/${organizationId}`, 'organization-overview'],
    [`/app/organizations/${organizationId}/members`, 'members'],
    [`/app/organizations/${organizationId}/installations`, 'installations'],
    [`/app/organizations/${organizationId}/products`, 'products'],
    ['/enroll', 'enroll'],
    ['/account/security', 'account-security'],
    ['/ruta-inexistente', 'not-found'],
  ] as const;

  await page.setViewportSize({ width: 1440, height: 900 });
  for (const [route, name] of routes) {
    await page.goto(route);
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: `${outDir}/portal-smoke-${name}.png`, fullPage: true });
  }

  for (const [width, height] of [
    [1024, 768],
    [768, 1024],
    [390, 844],
    [320, 720],
  ] as const) {
    await page.setViewportSize({ width, height });
    for (const [route, name] of [
      ['/app/organizations', 'organizations'],
      [`/app/organizations/${organizationId}`, 'organization-overview'],
      [`/app/organizations/${organizationId}/members`, 'members'],
      [`/app/organizations/${organizationId}/installations`, 'installations'],
    ] as const) {
      await page.goto(route);
      await page.waitForLoadState('networkidle');
      await page.screenshot({
        path: `${outDir}/portal-smoke-${name}-${width}.png`,
        fullPage: true,
      });
    }
  }

  // Sin desbordamiento horizontal del documento.
  await page.goto('/app/organizations');
  await page.waitForLoadState('networkidle');
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});

test('modal, tabla extensa y estado de error del Portal', async ({ page }) => {
  test.skip(!outDir, 'define HIQNET_VISUAL_DIR para generar capturas');
  await installMocks(page);
  await page.setViewportSize({ width: 1440, height: 900 });

  // Diálogo existente: alta de organización.
  await page.goto('/app/organizations');
  await page.waitForLoadState('networkidle');
  await page.getByRole('button', { name: 'Nueva organización' }).first().click();
  await page.getByRole('dialog').waitFor();
  await page.screenshot({ path: `${outDir}/portal-smoke-dialog.png` });
  await page.keyboard.press('Escape');

  // Tabla extensa: 24 organizaciones.
  const many = Array.from({ length: 24 }, (_, index) => ({
    id: `11111111-1111-4111-8111-0000000000${String(index).padStart(2, '0')}`,
    display_name: `Organización ${index + 1}`,
    status: 'active',
    created_at: '2026-01-03T00:00:00Z',
    role: index % 3 === 0 ? 'owner' : 'member',
  }));
  await page.unroute('**/api/v1/organizations**');
  await page.route('**/api/v1/organizations**', (route) =>
    route.fulfill(json({ items: many, next_cursor: null })),
  );
  await page.goto('/app/organizations');
  await page.waitForLoadState('networkidle');
  await page.screenshot({ path: `${outDir}/portal-smoke-long-table.png`, fullPage: true });

  // Estado de error real de una ruta con fallo de API.
  await page.unroute('**/api/v1/organizations**');
  await page.route('**/api/v1/organizations**', (route) =>
    route.fulfill(json({ code: 'storage_error', message: 'no disponible' }, 503)),
  );
  await page.goto('/app/organizations');
  await page.getByRole('alert').first().waitFor({ timeout: 10_000 });
  await page.screenshot({ path: `${outDir}/portal-smoke-error.png` });
});
