import { test } from '@playwright/test';

/**
 * Capturas de revisión visual del Portal bajo demanda.
 *
 * `HIQNET_VISUAL_DIR=/tmp/hiqnet-visual pnpm --filter @hiqnet/platform-portal e2e visual`
 *
 * El servidor de desarrollo del Portal se arranca manualmente (el config no
 * define webServer) o mediante las tareas full-stack existentes.
 */
const outDir = process.env.HIQNET_VISUAL_DIR;

const organizations = [
  {
    id: '11111111-1111-4111-8111-111111111111',
    display_name: 'Cafetería Central',
    status: 'active',
    created_at: '2026-01-03T00:00:00Z',
  },
  {
    id: '11111111-1111-4111-8111-111111111112',
    display_name: 'Distribuidora Norte',
    status: 'active',
    created_at: '2026-02-01T00:00:00Z',
  },
];

const viewports = [
  [1920, 1080],
  [1440, 900],
  [1024, 768],
  [768, 1024],
  [390, 844],
  [320, 720],
] as const;

test('capturas de login y organizaciones (bajo demanda)', async ({ page }) => {
  test.skip(!outDir, 'define HIQNET_VISUAL_DIR para generar capturas');

  await page.route('**/api/v1/organizations**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: organizations, next_cursor: null }),
    }),
  );

  await page.route('**/api/v1/auth/me', (route) =>
    route.fulfill({ status: 401, contentType: 'application/json', body: '{}' }),
  );
  for (const [width, height] of viewports) {
    await page.setViewportSize({ width, height });
    await page.goto('/login');
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: `${outDir}/portal-login-${width}.png` });
  }

  await page.unroute('**/api/v1/auth/me');
  await page.route('**/api/v1/auth/me', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        account_id: '22222222-2222-4222-8222-222222222222',
        email: 'cliente@example.test',
      }),
    }),
  );
  for (const [width, height] of viewports) {
    await page.setViewportSize({ width, height });
    await page.goto('/app/organizations');
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: `${outDir}/portal-orgs-${width}.png`, fullPage: true });
  }
});
