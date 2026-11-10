import type { components } from "./generated/openapi";

export type StaffMe = components["schemas"]["StaffMeResponse"];
export type Capability = NonNullable<StaffMe["capabilities"]>[number];
export type Product = components["schemas"]["ProductResponse"];
export type ProductPage = components["schemas"]["ProductListResponse"];
export type Project = components["schemas"]["ProjectResponse"];
export type ProjectPage = components["schemas"]["ProjectListResponse"];
export type ProductStatus = components["schemas"]["ProductStatusDto"];
export type ProductType = components["schemas"]["ProductTypeDto"];
export type CreateProductInput = components["schemas"]["CreateProductRequest"];
export type UpdateProductInput = components["schemas"]["UpdateProductRequest"];
export type OrganizationStatus = components["schemas"]["OrganizationStatusDto"];
export type AccountStatus = components["schemas"]["AccountStatusDto"];
export type ControlOrganization = components["schemas"]["ControlOrganizationSummaryResponse"];
export type ControlOrganizationPage = components["schemas"]["ControlOrganizationListResponse"];
export type ControlOrganizationDetail = components["schemas"]["ControlOrganizationDetailResponse"];
export type ControlOrganizationProduct = components["schemas"]["ControlOrganizationProductResponse"];
export type ControlOrganizationProductMutation = components["schemas"]["ControlOrganizationProductMutationResponse"];
export type CommercialPlanStatus = components["schemas"]["PlanStatusDto"];
export type CommercialPlan = components["schemas"]["PlanResponse"];
export type CommercialPlanPage = components["schemas"]["PlanListResponse"];
export type CommercialSubscriptionStatus = components["schemas"]["SubscriptionStatusDto"];
export type CommercialSubscription = components["schemas"]["SubscriptionResponse"];
export type CommercialSubscriptionPage = components["schemas"]["SubscriptionListResponse"];
export type CommercialSubscriptionDetail = components["schemas"]["SubscriptionDetailResponse"];
export type CommercialEntitlements = components["schemas"]["EntitlementsResponse"];
export type ControlAuditEvent = components["schemas"]["ControlAuditEventSummaryResponse"];
export type ControlAuditEventDetail = components["schemas"]["ControlAuditEventDetailResponse"];
export type ControlSecurityEvent = components["schemas"]["ControlSecurityEventSummaryResponse"];
export type ControlSecurityEventDetail = components["schemas"]["ControlSecurityEventDetailResponse"];
export type ControlStaffAuthEvent = components["schemas"]["ControlStaffAuthEventSummaryResponse"];
export type ControlStaffAuthEventDetail = components["schemas"]["ControlStaffAuthEventDetailResponse"];
export type ControlAccount = components["schemas"]["ControlAccountSummaryResponse"];
export type ControlAccountPage = components["schemas"]["ControlAccountListResponse"];
export type ControlAccountDetail = components["schemas"]["ControlAccountDetailResponse"];
export type InstallationAdministrativeStatus = components["schemas"]["AdministrativeStatusDto"];
export type InstallationIdentityStatus = components["schemas"]["IdentityStatusDto"];
export type InstallationRehostStatus = components["schemas"]["RehostStatusDto"];
export type ControlInstallation = components["schemas"]["ControlInstallationSummaryResponse"];
export type ControlInstallationPage = components["schemas"]["ControlInstallationListResponse"];
export type ControlInstallationDetail = components["schemas"]["ControlInstallationDetailResponse"];
export type InstallationRehostAuthorization = components["schemas"]["InstallationRehostResponse"];
export type RehostRequest = components["schemas"]["RehostRequest"];

export class ControlApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly code?: string,
  ) {
    super(message);
  }
}

const sessionExpiredEvent = "control:session-expired";
const sessionRestoredEvent = "control:session-restored";

function dispatchSessionEvent(name: string) {
  if (typeof window !== "undefined") window.dispatchEvent(new Event(name));
}

async function request<T>(path: string, init: RequestInit = {}, invalidateSession = true): Promise<T> {
  const response = await fetch(`/api/v1/control${path}`, {
    ...init,
    credentials: "include",
    headers: { "Content-Type": "application/json", ...init.headers },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { code?: string; message?: string } | null;
    if (invalidateSession && response.status === 401) dispatchSessionEvent(sessionExpiredEvent);
    throw new ControlApiError(response.status, body?.message ?? "No fue posible completar la solicitud.", body?.code);
  }
  return response.status === 204 ? (undefined as T) : (response.json() as Promise<T>);
}

export const controlClient = {
  login: async (email: string, password: string) => {
    const result = await request<void>("/auth/login", { method: "POST", body: JSON.stringify({ email, password }) }, false);
    dispatchSessionEvent(sessionRestoredEvent);
    return result;
  },
  logout: () => request<void>("/auth/logout", { method: "POST" }, false),
  me: () => request<StaffMe>("/auth/me", {}, false),
  products: (filters: { cursor?: string; status?: ProductStatus; type?: ProductType; limit?: number }) => {
    const search = new URLSearchParams();
    search.set("limit", String(filters.limit ?? 50));
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.type) search.set("type", filters.type);
    return request<ProductPage>(`/products?${search.toString()}`);
  },
  product: (key: string) => request<Product>(`/products/${encodeURIComponent(key)}`),
  createProduct: (body: CreateProductInput) =>
    request<Product>("/products", { method: "POST", body: JSON.stringify(body) }),
  updateProduct: (key: string, body: UpdateProductInput) =>
    request<Product>(`/products/${encodeURIComponent(key)}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  projects: (productKey: string) => request<ProjectPage>(`/projects?product_key=${encodeURIComponent(productKey)}&limit=50`),
  organizations: (filters: { cursor?: string; status?: OrganizationStatus; search?: string }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.search) search.set("search", filters.search);
    return request<ControlOrganizationPage>(`/organizations?${search.toString()}`);
  },
  organization: (id: string) => request<ControlOrganizationDetail>(`/organizations/${encodeURIComponent(id)}`),
  organizationProducts: (organizationId: string) =>
    request<{ items: ControlOrganizationProduct[] }>(
      `/organizations/${encodeURIComponent(organizationId)}/products`,
    ),
  activateOrganizationProduct: (organizationId: string, productId: string) =>
    request<ControlOrganizationProductMutation>(
      `/organizations/${encodeURIComponent(organizationId)}/products`,
      { method: "POST", body: JSON.stringify({ product_id: productId }) },
    ),
  setOrganizationProductStatus: (organizationId: string, productId: string, status: "active" | "inactive") =>
    request<ControlOrganizationProductMutation>(
      `/organizations/${encodeURIComponent(organizationId)}/products/${encodeURIComponent(productId)}`,
      { method: "PATCH", body: JSON.stringify({ status }) },
    ),
  plans: (filters: { productId?: string; status?: CommercialPlanStatus; cursor?: string; limit?: number }) => {
    const search = new URLSearchParams();
    search.set("limit", String(filters.limit ?? 50));
    if (filters.productId) search.set("product_id", filters.productId);
    if (filters.status) search.set("status", filters.status);
    if (filters.cursor) search.set("cursor", filters.cursor);
    return request<CommercialPlanPage>(`/plans?${search.toString()}`);
  },
  plan: (id: string) => request<CommercialPlan>(`/plans/${encodeURIComponent(id)}`),
  createPlan: (body: Record<string, unknown>) =>
    request<CommercialPlan>("/plans", { method: "POST", body: JSON.stringify(body) }),
  updatePlan: (id: string, body: Record<string, unknown>) =>
    request<CommercialPlan>(`/plans/${encodeURIComponent(id)}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  subscriptions: (filters: {
    organizationId?: string;
    productId?: string;
    status?: CommercialSubscriptionStatus;
    cursor?: string;
    limit?: number;
  }) => {
    const search = new URLSearchParams();
    search.set("limit", String(filters.limit ?? 50));
    if (filters.organizationId) search.set("organization_id", filters.organizationId);
    if (filters.productId) search.set("product_id", filters.productId);
    if (filters.status) search.set("status", filters.status);
    if (filters.cursor) search.set("cursor", filters.cursor);
    return request<CommercialSubscriptionPage>(`/subscriptions?${search.toString()}`);
  },
  subscription: (id: string) =>
    request<CommercialSubscriptionDetail>(`/subscriptions/${encodeURIComponent(id)}`),
  createSubscription: (body: Record<string, unknown>, idempotencyKey: string) =>
    request<CommercialSubscription>("/subscriptions", {
      method: "POST",
      body: JSON.stringify(body),
      headers: { "Idempotency-Key": idempotencyKey },
    }),
  suspendSubscription: (id: string, reason?: string) =>
    request<CommercialSubscription>(`/subscriptions/${encodeURIComponent(id)}/suspend`, {
      method: "POST",
      body: JSON.stringify({ reason }),
    }),
  reactivateSubscription: (id: string) =>
    request<CommercialSubscription>(`/subscriptions/${encodeURIComponent(id)}/reactivate`, {
      method: "POST",
      body: JSON.stringify({}),
    }),
  cancelSubscription: (id: string, reason?: string) =>
    request<CommercialSubscription>(`/subscriptions/${encodeURIComponent(id)}/cancel`, {
      method: "POST",
      body: JSON.stringify({ reason }),
    }),
  renewSubscription: (id: string, body: Record<string, unknown>, idempotencyKey: string) =>
    request<CommercialSubscription>(`/subscriptions/${encodeURIComponent(id)}/renew`, {
      method: "POST",
      body: JSON.stringify(body),
      headers: { "Idempotency-Key": idempotencyKey },
    }),
  entitlements: (organizationId: string, productId: string) =>
    request<CommercialEntitlements>(
      `/organizations/${encodeURIComponent(organizationId)}/entitlements?product_id=${encodeURIComponent(productId)}`,
    ),
  audit: (filters: { action?: string; result?: string; from?: string; to?: string; cursor?: string }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.action) search.set("action", filters.action);
    if (filters.result) search.set("result", filters.result);
    if (filters.from) search.set("from", filters.from);
    if (filters.to) search.set("to", filters.to);
    if (filters.cursor) search.set("cursor", filters.cursor);
    return request<{ items: ControlAuditEvent[]; next_cursor: string | null }>(
      `/audit?${search.toString()}`,
    );
  },
  auditEvent: (id: string) =>
    request<ControlAuditEventDetail>(`/audit/${encodeURIComponent(id)}`),
  securityEvents: (filters: {
    kind?: string;
    from?: string;
    to?: string;
    cursor?: string;
  }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.kind) search.set("kind", filters.kind);
    if (filters.from) search.set("from", filters.from);
    if (filters.to) search.set("to", filters.to);
    if (filters.cursor) search.set("cursor", filters.cursor);
    return request<{ items: ControlSecurityEvent[]; next_cursor: string | null }>(
      `/security-events?${search.toString()}`,
    );
  },
  securityEvent: (id: string) =>
    request<ControlSecurityEventDetail>(`/security-events/${encodeURIComponent(id)}`),
  staffAuthEvents: (filters: {
    action?: string;
    from?: string;
    to?: string;
    cursor?: string;
  }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.action) search.set("action", filters.action);
    if (filters.from) search.set("from", filters.from);
    if (filters.to) search.set("to", filters.to);
    if (filters.cursor) search.set("cursor", filters.cursor);
    return request<{ items: ControlStaffAuthEvent[]; next_cursor: string | null }>(
      `/staff-auth-events?${search.toString()}`,
    );
  },
  staffAuthEvent: (id: string) =>
    request<ControlStaffAuthEventDetail>(`/staff-auth-events/${encodeURIComponent(id)}`),
  accounts: (filters: { cursor?: string; status?: AccountStatus; search?: string }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.search) search.set("search", filters.search);
    return request<ControlAccountPage>(`/accounts?${search.toString()}`);
  },
  account: (id: string) => request<ControlAccountDetail>(`/accounts/${encodeURIComponent(id)}`),
  installations: (filters: {
    cursor?: string;
    status?: InstallationAdministrativeStatus;
    identityStatus?: InstallationIdentityStatus;
    organizationId?: string;
    productId?: string;
    search?: string;
    limit?: number;
  }) => {
    const search = new URLSearchParams();
    search.set("limit", String(filters.limit ?? 50));
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.identityStatus) search.set("identity_status", filters.identityStatus);
    if (filters.organizationId) search.set("organization_id", filters.organizationId);
    if (filters.productId) search.set("product_id", filters.productId);
    if (filters.search) search.set("search", filters.search);
    return request<ControlInstallationPage>(`/installations?${search.toString()}`);
  },
  installation: (id: string) => request<ControlInstallationDetail>(`/installations/${encodeURIComponent(id)}`),
  suspendInstallation: (id: string) =>
    request<ControlInstallationDetail>(`/installations/${encodeURIComponent(id)}/suspend`, { method: "POST" }),
  reactivateInstallation: (id: string) =>
    request<ControlInstallationDetail>(`/installations/${encodeURIComponent(id)}/reactivate`, { method: "POST" }),
  authorizeInstallationRehost: (id: string, body: RehostRequest) =>
    request<InstallationRehostAuthorization>(`/installations/${encodeURIComponent(id)}/rehost`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
};

export const controlSessionEvents = { expired: sessionExpiredEvent, restored: sessionRestoredEvent };
