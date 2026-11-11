import { expect, test } from "@playwright/test";

const outDir = process.env.HIQNET_VISUAL_DIR;

const staff = {
  account_id: "00000000-0000-4000-8000-000000000001",
  display_name: "Control Admin",
  email: "admin@hiqnet.test",
  role: "platform_admin",
  capabilities: [
    "products.read",
    "products.manage",
    "organizations.read",
    "organizations.manage",
    "accounts.read",
    "installations.read",
    "installations.manage",
    "installations.rehost",
    "subscriptions.read",
    "subscriptions.manage",
    "audit.read",
    "security.read",
  ],
};

const org = { id: "11111111-1111-4111-8111-111111111111", display_name: "Cafetería Central", status: "active" };
const product = {
  id: "00000000-0000-4000-8000-000000000010",
  key: "pos",
  name: "HiQNet POS",
  description: "Punto de venta local-first.",
  status: "active",
  product_type: "desktop",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-02-10T00:00:00Z",
};
const plan = {
  id: "44444444-4444-4444-8444-444444444441",
  product: { id: product.id, key: "pos", name: "HiQNet POS" },
  key: "local",
  name: "Local",
  description: "Operación local completa.",
  status: "active",
  is_trial: false,
  features: { catalog: true, sales: true },
  limits: { seats: 5 },
  subscription_count: 12,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-02-01T00:00:00Z",
};
const subscription = {
  id: "55555555-5555-4555-8555-555555555551",
  organization_id: org.id,
  product_id: product.id,
  plan: { id: plan.id, key: "local", name: "Local", is_trial: false },
  status: "active",
  is_trial: false,
  starts_at: "2026-01-01T00:00:00Z",
  ends_at: "2027-01-01T00:00:00Z",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  suspended_at: null,
  cancelled_at: null,
  renewed_from_subscription_id: null,
};
const installation = {
  id: "33333333-3333-4333-8333-333333333331",
  device_label: "Caja mostrador",
  organization: org,
  product: { id: product.id, key: "pos", name: "HiQNet POS" },
  administrative_status: "active",
  identity_status: "active",
  registered_at: "2026-01-10T10:00:00Z",
  enrolled_at: "2026-01-10T10:05:00Z",
  last_known_activity_at: "2026-02-12T09:30:00Z",
  identity_generation: 1,
  binding_active: true,
  fingerprint: "ab12cd34ef56ab78cd90ef12ab34cd56ef78ab90cd12ef34ab56cd78ef90ab12",
};
const account = {
  id: "22222222-2222-4222-8222-222222222222",
  email: "cliente@example.test",
  display_name: "Cliente Ejemplo",
  status: "active",
  created_at: "2026-01-02T00:00:00Z",
};
const auditEvent = {
  id: "66666666-6666-4666-8666-666666666661",
  action: "product_updated",
  actor: { account_id: staff.account_id, display_name: staff.display_name, email: staff.email },
  organization: { id: org.id, display_name: org.display_name },
  product: { id: product.id, key: product.key, name: product.name },
  resource_id: product.id,
  resource_label: "pos",
  resource_type: "product",
  result: "success",
  occurred_at: "2026-02-10T12:00:00Z",
};
const securityEvent = {
  id: "77777777-7777-4777-8777-777777777771",
  kind: "enrollment_proof_rejected",
  account: null,
  installation: { id: installation.id, device_label: installation.device_label },
  organization: { id: org.id, display_name: org.display_name },
  occurred_at: "2026-02-11T08:00:00Z",
};

function json(body: unknown, status = 200) {
  return { status, contentType: "application/json", body: JSON.stringify(body) };
}

async function installMocks(page: import("@playwright/test").Page) {
  await page.route("**/api/v1/control/auth/me", (route) => route.fulfill(json(staff)));
  await page.route("**/api/v1/control/products**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/products")) {
      return route.fulfill(json({ items: [product], next_cursor: null }));
    }
    return route.fulfill(json(product));
  });
  await page.route("**/api/v1/control/projects**", (route) => route.fulfill(json({ items: [], next_cursor: null })));
  await page.route("**/api/v1/control/plans**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/plans")) return route.fulfill(json({ items: [plan], next_cursor: null }));
    return route.fulfill(json(plan));
  });
  await page.route("**/api/v1/control/subscriptions**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/subscriptions")) {
      return route.fulfill(json({ items: [subscription], next_cursor: null }));
    }
    return route.fulfill(json({ subscription, events: [] }));
  });
  await page.route("**/api/v1/control/installations**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/installations")) {
      return route.fulfill(json({ items: [installation], next_cursor: null }));
    }
    return route.fulfill(json({ summary: installation, current_binding: null, rehost: null }));
  });
  await page.route("**/api/v1/control/organizations**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/organizations")) {
      return route.fulfill(json({ items: [org], next_cursor: null }));
    }
    return route.fulfill(
      json({
        ...org,
        memberships: [{ ...account, role: "owner", joined_at: "2026-01-02T00:00:00Z", revoked_at: null }],
        products: [],
      }),
    );
  });
  await page.route("**/api/v1/control/accounts**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/accounts")) {
      return route.fulfill(json({ items: [account], next_cursor: null }));
    }
    return route.fulfill(json({ ...account, memberships: [] }));
  });
  await page.route("**/api/v1/control/audit**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/audit")) return route.fulfill(json({ items: [auditEvent], next_cursor: null }));
    return route.fulfill(json({ ...auditEvent, metadata: {} }));
  });
  await page.route("**/api/v1/control/security-events**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/security-events")) {
      return route.fulfill(json({ items: [securityEvent], next_cursor: null }));
    }
    return route.fulfill(json({ ...securityEvent, metadata: {} }));
  });
  await page.route("**/api/v1/control/staff-auth-events**", (route) => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/staff-auth-events")) {
      return route.fulfill(json({ items: [], next_cursor: null }));
    }
    return route.fulfill(json({}));
  });
}

const routes = [
  ["/control/products", "products"],
  ["/control/products/pos", "product-detail"],
  ["/control/plans", "plans"],
  [`/control/plans/${plan.id}`, "plan-detail"],
  ["/control/subscriptions", "subscriptions"],
  [`/control/subscriptions/${subscription.id}`, "subscription-detail"],
  ["/control/installations", "installations"],
  [`/control/installations/${installation.id}`, "installation-detail"],
  ["/control/organizations", "organizations"],
  [`/control/organizations/${org.id}`, "organization-detail"],
  ["/control/accounts", "accounts"],
  [`/control/accounts/${account.id}`, "account-detail"],
  ["/control/audit", "audit"],
  [`/control/audit/${auditEvent.id}`, "audit-detail"],
  ["/control/security", "security"],
  [`/control/security/${securityEvent.id}`, "security-detail"],
  ["/control/security/staff-auth", "staff-auth"],
] as const;

test("smoke visual de todas las rutas del Control Center", async ({ page }) => {
  test.skip(!outDir, "define HIQNET_VISUAL_DIR para generar capturas");
  await installMocks(page);

  await page.setViewportSize({ width: 1440, height: 900 });
  for (const [route, name] of routes) {
    await page.goto(route);
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: `${outDir}/smoke-${name}.png`, fullPage: true });
  }

  // Foco por teclado: el primer Tab debe mostrar un foco visible en el skip link.
  await page.goto("/control/products");
  await page.waitForLoadState("networkidle");
  await page.keyboard.press("Tab");
  await page.screenshot({ path: `${outDir}/smoke-focus.png` });

  // Responsive clave.
  for (const [width, height] of [
    [1024, 768],
    [768, 1024],
    [390, 844],
    [320, 720],
  ] as const) {
    await page.setViewportSize({ width, height });
    for (const [route, name] of [
      ["/control/products", "products"],
      ["/control/installations", "installations"],
      ["/control/plans", "plans"],
      [`/control/installations/${installation.id}`, "installation-detail"],
    ] as const) {
      await page.goto(route);
      await page.waitForLoadState("networkidle");
      await page.screenshot({ path: `${outDir}/smoke-${name}-${width}.png`, fullPage: true });
    }
  }
});

test("modal y estado de error", async ({ page }) => {
  test.skip(!outDir, "define HIQNET_VISUAL_DIR para generar capturas");
  await installMocks(page);
  await page.setViewportSize({ width: 1440, height: 900 });

  // Modal existente: acción destructiva en el detalle de instalación.
  await page.goto(`/control/installations/${installation.id}`);
  await page.waitForLoadState("networkidle");
  const suspend = page.getByRole("button", { name: /^suspend$/i }).first();
  if (await suspend.count()) {
    await suspend.click();
    await page.screenshot({ path: `${outDir}/smoke-modal.png` });
    await page.keyboard.press("Escape");
  }

  // Estado de error real de una ruta con fallo de API.
  await page.unroute("**/api/v1/control/products**");
  await page.route("**/api/v1/control/products**", (route) =>
    route.fulfill(json({ code: "storage_error", message: "no disponible" }, 503)),
  );
  await page.goto("/control/products");
  await page.getByRole("alert").first().waitFor({ timeout: 10_000 });
  await page.screenshot({ path: `${outDir}/smoke-error.png` });

  // Tabla extensa.
  await page.unroute("**/api/v1/control/products**");
  const many = Array.from({ length: 24 }, (_, index) => ({
    ...product,
    id: `00000000-0000-4000-8000-0000000000${String(index).padStart(2, "0")}`,
    key: `producto-${index}`,
    name: `Producto ${index + 1}`,
    status: index % 3 === 0 ? "maintenance" : "active",
  }));
  await page.route("**/api/v1/control/products**", (route) =>
    route.fulfill(json({ items: many, next_cursor: null })),
  );
  await page.goto("/control/products");
  await page.waitForLoadState("networkidle");
  await page.screenshot({ path: `${outDir}/smoke-long-table.png`, fullPage: true });

  // Sin desbordamiento horizontal del documento en escritorio.
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});
