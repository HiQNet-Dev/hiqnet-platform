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
  id: '33333333-3333-4333-8333-333333333331',
  device_label: 'Caja mostrador',
  organization: { id: organizationId, display_name: organization.display_name, status: 'active' },
  product: { id: '00000000-0000-4000-8000-000000000010', key: 'pos', name: 'HiQNet POS' },
  administrative_status: 'active',
  identity_status: 'active',
  registered_at: '2026-01-10T10:00:00Z',
  enrolled_at: '2026-01-10T10:05:00Z',
  last_known_activity_at: '2026-02-12T09:30:00Z',
  identity_generation: 1,
  binding_active: true,
  fingerprint: 'ab12cd34ef56ab78cd90ef12ab34cd56ef78ab90cd12ef34ab56cd78ef90ab12',
};

function json(body: unknown, status = 200) {
  return { status, contentType: 'application/json', body: JSON.stringify(body) };
}

async function installMocks(page: Page) {
  await page.route('**/api/v1/auth/me', (route) => route.fulfill(json(account)));
  await page.route('**/api/v1/organizations', (route) =>
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
