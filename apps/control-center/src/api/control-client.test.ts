import { afterEach, describe, expect, it, vi } from "vitest";
import { controlClient } from "./control-client";

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("controlClient de installations", () => {
  it("construye la query del contrato y envía cookies de sesión", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ items: [], next_cursor: null }));
    vi.stubGlobal("fetch", fetchMock);

    await controlClient.installations({
      status: "suspended",
      identityStatus: "active",
      organizationId: "org-1",
      productId: "product-1",
      search: "caja",
      cursor: "cursor-1",
      limit: 25,
    });

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(
      "/api/v1/control/installations?limit=25&cursor=cursor-1&status=suspended&identity_status=active&organization_id=org-1&product_id=product-1&search=caja",
    );
    expect(init.credentials).toBe("include");
  });

  it("usa exclusivamente los cinco endpoints Control aprobados", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({}))
      .mockResolvedValueOnce(jsonResponse({}))
      .mockResolvedValueOnce(jsonResponse({}))
      .mockResolvedValueOnce(jsonResponse({}));
    vi.stubGlobal("fetch", fetchMock);

    await controlClient.installation("id");
    await controlClient.suspendInstallation("id");
    await controlClient.reactivateInstallation("id");
    await controlClient.authorizeInstallationRehost("id", {
      enrollment_request_id: "request-1",
      reason: "reemplazo",
    });

    const calls = fetchMock.mock.calls.map(([url, init]) => [
      url,
      (init as RequestInit | undefined)?.method ?? "GET",
    ]);
    expect(calls).toEqual([
      ["/api/v1/control/installations/id", "GET"],
      ["/api/v1/control/installations/id/suspend", "POST"],
      ["/api/v1/control/installations/id/reactivate", "POST"],
      ["/api/v1/control/installations/id/rehost", "POST"],
    ]);
  });

  it("expone el código ErrorResponse para distinguir conflictos 409", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(jsonResponse({ code: "rehost_conflict", message: "x" }, 409)),
    );
    await expect(controlClient.installation("id")).rejects.toMatchObject({
      status: 409,
      code: "rehost_conflict",
    });
  });
});

describe("controlClient de OrganizationProduct", () => {
  it("usa GET, POST y PATCH del contrato sin DELETE", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ items: [] }))
      .mockResolvedValueOnce(jsonResponse({ changed: true }))
      .mockResolvedValueOnce(jsonResponse({ changed: true }));
    vi.stubGlobal("fetch", fetchMock);

    await controlClient.organizationProducts("org-1");
    await controlClient.activateOrganizationProduct("org-1", "product-1");
    await controlClient.setOrganizationProductStatus("org-1", "product-1", "inactive");

    const calls = fetchMock.mock.calls.map(([url, init]) => [
      url,
      (init as RequestInit | undefined)?.method ?? "GET",
      (init as RequestInit | undefined)?.body,
    ]);
    expect(calls).toEqual([
      ["/api/v1/control/organizations/org-1/products", "GET", undefined],
      [
        "/api/v1/control/organizations/org-1/products",
        "POST",
        JSON.stringify({ product_id: "product-1" }),
      ],
      [
        "/api/v1/control/organizations/org-1/products/product-1",
        "PATCH",
        JSON.stringify({ status: "inactive" }),
      ],
    ]);
  });
});
