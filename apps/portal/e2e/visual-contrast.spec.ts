import { expect, test } from '@playwright/test';

// Verificación de contraste WCAG AA sobre el tema de marca del Portal.
// No depende de capturas: lee colores computados y calcula la relación.
const outDir = process.env.HIQNET_VISUAL_DIR;

const account = {
  account_id: '22222222-2222-4222-8222-222222222222',
  id: '22222222-2222-4222-8222-222222222222',
  email: 'cliente@example.test',
  display_name: 'Cliente Ejemplo',
};

test('contraste AA de textos e indicadores clave', async ({ page }) => {
  await page.route('**/api/v1/auth/me', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(account),
    }),
  );

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/account/security');
  await page.waitForLoadState('networkidle');

  const results = await page.evaluate(() => {
    const parse = (value: string) => {
      const match = value.match(/rgba?\(([^)]+)\)/);
      if (!match) return null;
      const [r, g, b, a = '1'] = match[1].split(',').map((part) => part.trim());
      return { r: Number(r), g: Number(g), b: Number(b), a: Number(a) };
    };
    const luminance = (color: { r: number; g: number; b: number }) => {
      const channel = (value: number) => {
        const c = value / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
    };
    const effective = (element: Element) => {
      let background: ReturnType<typeof parse> = null;
      let node: Element | null = element;
      while (node) {
        const candidate = parse(getComputedStyle(node).backgroundColor);
        if (candidate && candidate.a > 0.6) {
          background = candidate;
          break;
        }
        node = node.parentElement;
      }
      return {
        fg: parse(getComputedStyle(element).color),
        bg: background ?? { r: 13, g: 15, b: 18, a: 1 },
      };
    };
    const ratio = (fg: ReturnType<typeof parse>, bg: ReturnType<typeof parse>) => {
      if (!fg || !bg) return 0;
      const l1 = luminance(fg);
      const l2 = luminance(bg);
      const [light, dark] = l1 > l2 ? [l1, l2] : [l2, l1];
      return (light + 0.05) / (dark + 0.05);
    };
    const selectors = [
      '.page-header h1',
      '.page-header__description',
      '.panel__heading h2',
      '.sidebar__link',
      '.sidebar__link--active',
      '.button--primary',
      '.button--secondary',
      '.button--danger',
      '.form-field__label',
      '.form-field__hint',
      '.eyebrow',
    ];
    return selectors.flatMap((selector) => {
      const element = document.querySelector(selector);
      if (!element) return [];
      const { fg, bg } = effective(element);
      return [{ selector, ratio: Number(ratio(fg, bg).toFixed(2)) }];
    });
  });

  for (const { selector, ratio } of results) {
    expect(ratio, `contraste insuficiente en ${selector} (${ratio})`).toBeGreaterThanOrEqual(4.5);
  }

  if (outDir) {
    await page.screenshot({ path: `${outDir}/portal-contrast.png`, fullPage: true });
  }
});
