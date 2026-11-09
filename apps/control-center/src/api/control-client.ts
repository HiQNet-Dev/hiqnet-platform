import type { components } from "./generated/openapi";

export type StaffMe = components["schemas"]["StaffMeResponse"];
export type Capability = NonNullable<StaffMe["capabilities"]>[number];
export type Product = components["schemas"]["ProductResponse"];
export type ProductPage = components["schemas"]["ProductListResponse"];
export type Project = components["schemas"]["ProjectResponse"];
export type ProjectPage = components["schemas"]["ProjectListResponse"];
export type ProductStatus = components["schemas"]["ProductStatusDto"];
export type ProductType = components["schemas"]["ProductTypeDto"];

export class ControlApiError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`/api/v1/control${path}`, {
    ...init,
    credentials: "include",
    headers: { "Content-Type": "application/json", ...init.headers },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as { message?: string } | null;
    throw new ControlApiError(response.status, body?.message ?? "No fue posible completar la solicitud.");
  }
  return response.status === 204 ? (undefined as T) : (response.json() as Promise<T>);
}

export const controlClient = {
  login: (email: string, password: string) => request<void>("/auth/login", { method: "POST", body: JSON.stringify({ email, password }) }),
  logout: () => request<void>("/auth/logout", { method: "POST" }),
  me: () => request<StaffMe>("/auth/me"),
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
};
