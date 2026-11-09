import type { components } from "./generated/openapi";

export type StaffMe = components["schemas"]["StaffMeResponse"];
export type Capability = NonNullable<StaffMe["capabilities"]>[number];
export type Product = components["schemas"]["ProductResponse"];
export type ProductPage = components["schemas"]["ProductListResponse"];
export type Project = components["schemas"]["ProjectResponse"];
export type ProjectPage = components["schemas"]["ProjectListResponse"];
export type ProductStatus = components["schemas"]["ProductStatusDto"];
export type ProductType = components["schemas"]["ProductTypeDto"];
export type OrganizationStatus = components["schemas"]["OrganizationStatusDto"];
export type AccountStatus = components["schemas"]["AccountStatusDto"];
export type ControlOrganization = components["schemas"]["ControlOrganizationSummaryResponse"];
export type ControlOrganizationPage = components["schemas"]["ControlOrganizationListResponse"];
export type ControlOrganizationDetail = components["schemas"]["ControlOrganizationDetailResponse"];
export type ControlAccount = components["schemas"]["ControlAccountSummaryResponse"];
export type ControlAccountPage = components["schemas"]["ControlAccountListResponse"];
export type ControlAccountDetail = components["schemas"]["ControlAccountDetailResponse"];

export class ControlApiError extends Error {
  constructor(public readonly status: number, message: string) {
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
    const body = (await response.json().catch(() => null)) as { message?: string } | null;
    if (invalidateSession && response.status === 401) dispatchSessionEvent(sessionExpiredEvent);
    throw new ControlApiError(response.status, body?.message ?? "No fue posible completar la solicitud.");
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
  products: (filters: { cursor?: string; status?: ProductStatus; type?: ProductType }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.type) search.set("type", filters.type);
    return request<ProductPage>(`/products?${search.toString()}`);
  },
  product: (key: string) => request<Product>(`/products/${encodeURIComponent(key)}`),
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
  accounts: (filters: { cursor?: string; status?: AccountStatus; search?: string }) => {
    const search = new URLSearchParams();
    search.set("limit", "50");
    if (filters.cursor) search.set("cursor", filters.cursor);
    if (filters.status) search.set("status", filters.status);
    if (filters.search) search.set("search", filters.search);
    return request<ControlAccountPage>(`/accounts?${search.toString()}`);
  },
  account: (id: string) => request<ControlAccountDetail>(`/accounts/${encodeURIComponent(id)}`),
};

export const controlSessionEvents = { expired: sessionExpiredEvent, restored: sessionRestoredEvent };
