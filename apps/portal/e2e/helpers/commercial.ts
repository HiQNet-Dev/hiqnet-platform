const API_BASE = process.env.PLATFORM_API_URL ?? 'http://127.0.0.1:8080';
const PORTAL_ORIGIN = process.env.PORTAL_ORIGIN ?? 'http://127.0.0.1:5173';

async function json<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`API ${response.status}: ${await response.text()}`);
  }
  return (await response.json()) as T;
}

function cookieFrom(response: Response): string {
  const raw = response.headers.getSetCookie()[0];
  if (!raw) throw new Error('La API no devolvió cookie de sesión');
  return raw.split(';')[0];
}

export async function staffLogin(email: string, password: string): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/control/auth/login`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Origin: PORTAL_ORIGIN,
    },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) {
    throw new Error(`staff login falló con ${response.status}`);
  }
  return cookieFrom(response);
}

export async function commercialLogin(email: string, password: string): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/auth/login`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Origin: PORTAL_ORIGIN,
    },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) {
    throw new Error(`commercial login falló con ${response.status}`);
  }
  return cookieFrom(response);
}

export function staffHeaders(cookie: string) {
  return {
    'Content-Type': 'application/json',
    Origin: PORTAL_ORIGIN,
    Cookie: cookie,
  };
}

export function commercialHeaders(cookie: string) {
  return {
    'Content-Type': 'application/json',
    Origin: PORTAL_ORIGIN,
    Cookie: cookie,
  };
}

export async function createProduct(cookie: string) {
  const key = `e2e-${crypto.randomUUID().slice(0, 8)}`;
  return json<{ id: string; key: string; name: string }>(
    await fetch(`${API_BASE}/api/v1/control/products`, {
      method: 'POST',
      headers: staffHeaders(cookie),
      body: JSON.stringify({
        key,
        name: `Producto E2E ${key}`,
        description: 'Producto sintético no-POS',
        status: 'active',
        type: 'saas',
      }),
    }),
  );
}

export async function createCommercialOrganization(cookie: string) {
  return json<{ id: string; display_name: string }>(
    await fetch(`${API_BASE}/api/v1/organizations`, {
      method: 'POST',
      headers: commercialHeaders(cookie),
      body: JSON.stringify({ display_name: `Comercial E2E ${Date.now()}` }),
    }),
  );
}

export async function activateOrganizationProduct(
  cookie: string,
  organizationId: string,
  productId: string,
) {
  return json(
    await fetch(`${API_BASE}/api/v1/control/organizations/${organizationId}/products`, {
      method: 'POST',
      headers: staffHeaders(cookie),
      body: JSON.stringify({ product_id: productId }),
    }),
  );
}

export async function createPlan(cookie: string, productId: string, key = 'pro') {
  return json<{ id: string; key: string; name: string }>(
    await fetch(`${API_BASE}/api/v1/control/plans`, {
      method: 'POST',
      headers: staffHeaders(cookie),
      body: JSON.stringify({
        product_id: productId,
        key,
        name: `Plan ${key}`,
        status: 'active',
        features: { sync: true },
        limits: { seats: 5 },
      }),
    }),
  );
}

export async function createSubscription(
  cookie: string,
  organizationId: string,
  productId: string,
  planId: string,
) {
  const starts = new Date();
  const ends = new Date();
  ends.setDate(ends.getDate() + 30);
  return json<{ id: string; status: string }>(
    await fetch(`${API_BASE}/api/v1/control/subscriptions`, {
      method: 'POST',
      headers: {
        ...staffHeaders(cookie),
        'Idempotency-Key': crypto.randomUUID(),
      },
      body: JSON.stringify({
        organization_id: organizationId,
        product_id: productId,
        plan_id: planId,
        starts_at: starts.toISOString(),
        ends_at: ends.toISOString(),
      }),
    }),
  );
}

export async function transitionSubscription(
  cookie: string,
  subscriptionId: string,
  action: 'suspend' | 'reactivate' | 'cancel',
) {
  return json(
    await fetch(`${API_BASE}/api/v1/control/subscriptions/${subscriptionId}/${action}`, {
      method: 'POST',
      headers: staffHeaders(cookie),
      body: JSON.stringify(action === 'suspend' ? { reason: 'e2e' } : {}),
    }),
  );
}

export async function getEntitlements(cookie: string, organizationId: string, productId: string) {
  return json<{ effective: boolean; reason: string }>(
    await fetch(
      `${API_BASE}/api/v1/control/organizations/${organizationId}/entitlements?product_id=${productId}`,
      { headers: staffHeaders(cookie) },
    ),
  );
}

export async function getPortalProduct(cookie: string, organizationId: string, productId: string) {
  return json<{ entitlements: { effective: boolean; reason: string } }>(
    await fetch(`${API_BASE}/api/v1/organizations/${organizationId}/products/${productId}`, {
      headers: commercialHeaders(cookie),
    }),
  );
}

export async function expectOtherTenantNotFound(cookie: string) {
  const response = await fetch(
    `${API_BASE}/api/v1/organizations/00000000-0000-4000-8000-000000000000/products`,
    { headers: commercialHeaders(cookie) },
  );
  if (response.status !== 404) {
    throw new Error(`cross-tenant debía ser 404, fue ${response.status}`);
  }
}
