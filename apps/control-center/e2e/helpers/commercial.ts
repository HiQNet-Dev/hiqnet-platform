const API_BASE = process.env.PLATFORM_API_URL ?? "http://127.0.0.1:8080";
const CONTROL_ORIGIN = process.env.CONTROL_ORIGIN ?? "http://127.0.0.1:5174";

async function json<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`API ${response.status}: ${await response.text()}`);
  }
  return (await response.json()) as T;
}

function cookieFrom(response: Response): string {
  const raw = response.headers.getSetCookie()[0];
  if (!raw) throw new Error("La API no devolvió cookie de sesión");
  return raw.split(";")[0];
}

export async function staffLogin(email: string, password: string): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/control/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: CONTROL_ORIGIN },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) throw new Error(`staff login falló con ${response.status}`);
  return cookieFrom(response);
}

export async function commercialLogin(email: string, password: string): Promise<string> {
  const response = await fetch(`${API_BASE}/api/v1/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: CONTROL_ORIGIN },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) throw new Error(`commercial login falló con ${response.status}`);
  return cookieFrom(response);
}

function headers(cookie: string) {
  return { "Content-Type": "application/json", Origin: CONTROL_ORIGIN, Cookie: cookie };
}

export async function createProduct(cookie: string) {
  const key = `cc-${crypto.randomUUID().slice(0, 8)}`;
  return json<{ id: string; key: string; name: string }>(
    await fetch(`${API_BASE}/api/v1/control/products`, {
      method: "POST",
      headers: headers(cookie),
      body: JSON.stringify({
        key,
        name: `Producto CC ${key}`,
        description: "Producto sintético no-POS del E2E de Control Center",
        status: "active",
        type: "saas",
      }),
    }),
  );
}

export async function createCommercialOrganization(cookie: string) {
  return json<{ id: string; display_name: string }>(
    await fetch(`${API_BASE}/api/v1/organizations`, {
      method: "POST",
      headers: headers(cookie),
      body: JSON.stringify({ display_name: `Organización CC ${Date.now()}` }),
    }),
  );
}

/** Organización sembrada y propiedad de la cuenta comercial E2E. */
export const SEEDED_ORGANIZATION_ID =
  process.env.PORTAL_E2E_ORGANIZATION_ID ?? "11111111-1111-4111-8111-111111111111";

export async function getOrganization(cookie: string, organizationId: string) {
  return json<{ id: string; display_name: string }>(
    await fetch(`${API_BASE}/api/v1/control/organizations/${organizationId}`, {
      headers: headers(cookie),
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
      method: "POST",
      headers: headers(cookie),
      body: JSON.stringify({ product_id: productId }),
    }),
  );
}

export async function createPlan(cookie: string, productId: string) {
  const key = `ccplan-${crypto.randomUUID().slice(0, 8)}`;
  return json<{ id: string; key: string; name: string }>(
    await fetch(`${API_BASE}/api/v1/control/plans`, {
      method: "POST",
      headers: headers(cookie),
      body: JSON.stringify({
        product_id: productId,
        key,
        name: `Plan CC ${key}`,
        status: "active",
        features: { sync: true },
        limits: { seats: 3 },
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
      method: "POST",
      headers: { ...headers(cookie), "Idempotency-Key": crypto.randomUUID() },
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

export async function getEntitlements(cookie: string, organizationId: string, productId: string) {
  return json<{ effective: boolean; reason: string }>(
    await fetch(
      `${API_BASE}/api/v1/control/organizations/${organizationId}/entitlements?product_id=${productId}`,
      { headers: headers(cookie) },
    ),
  );
}
