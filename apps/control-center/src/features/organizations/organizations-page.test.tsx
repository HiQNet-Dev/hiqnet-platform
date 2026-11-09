import { describe, expect, it } from "vitest";
import { applyDirectoryFilter, withDirectoryCursor } from "../directory/filters";
import { formatDate, formatDateTime } from "../../lib/format-date";
import { controlModules } from "../../modules/registry";

describe("registry del Control Center", () => {
  it("registra Organizations, Accounts e Installations con sus capabilities independientes", () => {
    const organizations = controlModules.find((module) => module.id === "organizations");
    const accounts = controlModules.find((module) => module.id === "accounts");
    const installations = controlModules.find((module) => module.id === "installations");
    expect(organizations?.requiredCapability).toBe("organizations.read");
    expect(accounts?.requiredCapability).toBe("accounts.read");
    expect(installations?.requiredCapability).toBe("installations.read");
    expect(organizations?.route).toBe("/control/organizations");
    expect(accounts?.route).toBe("/control/accounts");
    expect(installations?.route).toBe("/control/installations");
  });
});

describe("formatter RFC3339", () => {
  it("formatea timestamps válidos y degrada valores inválidos", () => {
    expect(formatDate("2026-01-02T00:00:00Z")).not.toBe("—");
    expect(formatDateTime("2026-01-02T00:00:00Z")).not.toBe("—");
    expect(formatDateTime(null)).toBe("—");
    expect(formatDate("no-es-fecha")).toBe("—");
  });
});

describe("filtros del directorio", () => {
  it("cambiar status o search elimina el cursor previo", () => {
    const initial = new URLSearchParams("status=active&cursor=abc123&search=pos");
    const byStatus = applyDirectoryFilter(initial, { status: "archived" });
    expect(byStatus.get("status")).toBe("archived");
    expect(byStatus.get("cursor")).toBeNull();

    const bySearch = applyDirectoryFilter(initial, { search: "  gael  " });
    expect(bySearch.get("search")).toBe("gael");
    expect(bySearch.get("cursor")).toBeNull();

    const cleared = applyDirectoryFilter(initial, { search: "   " });
    expect(cleared.get("search")).toBeNull();
    expect(cleared.get("cursor")).toBeNull();
  });

  it("la paginación conserva filtros y solo mueve el cursor", () => {
    const initial = new URLSearchParams("status=active&search=pos");
    const next = withDirectoryCursor(initial, "next-cursor");
    expect(next.get("status")).toBe("active");
    expect(next.get("search")).toBe("pos");
    expect(next.get("cursor")).toBe("next-cursor");
    expect(withDirectoryCursor(next, null).get("cursor")).toBeNull();
  });

  it("los filtros de installations usan los nombres del contrato y resetean el cursor", () => {
    const initial = new URLSearchParams("status=active&cursor=stale&search=pos");
    const next = applyDirectoryFilter(initial, {
      identityStatus: "unenrolled",
      organizationId: "org-1",
      productId: "product-1",
    });
    expect(next.get("identity_status")).toBe("unenrolled");
    expect(next.get("organization_id")).toBe("org-1");
    expect(next.get("product_id")).toBe("product-1");
    expect(next.get("status")).toBe("active");
    expect(next.get("cursor")).toBeNull();
    const cleared = applyDirectoryFilter(next, { identityStatus: "" });
    expect(cleared.get("identity_status")).toBeNull();
    expect(cleared.get("cursor")).toBeNull();
  });
});
