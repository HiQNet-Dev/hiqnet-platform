import { expect, test, type Page } from "@playwright/test";

const organization = {
  id: "00000000-0000-4000-8000-000000000020",
  display_name: "Org Alpha",
  status: "active",
};
const product = {
  id: "00000000-0000-4000-8000-000000000010",
  key: "pos",
  name: "HiQNet POS",
};
const fingerprint = `a12f${"0".repeat(56)}93bc`;

const activeInstallation = {
  id: "00000000-0000-4000-8000-000000000100",
  device_label: "Caja Centro",
  organization,
  product,
  administrative_status: "active",
  identity_status: "active",
  registered_at: "2026-01-01T00:00:00Z",
  enrolled_at: "2026-01-02T00:00:00Z",
  last_known_activity_at: "2026-01-10T12:00:00Z",
  identity_generation: 1,
  binding_active: true,
  fingerprint,
};
const suspendedInstallation = {
  ...activeInstallation,
  id: "00000000-0000-4000-8000-000000000101",
  device_label: "Caja Norte",
  administrative_status: "suspended",
};
const retiredInstallation = {
  ...activeInstallation,
  id: "00000000-0000-4000-8000-000000000102",
  device_label: "Caja Retirada",
  identity_status: "retired",
  binding_active: false,
  fingerprint: null,
};
const postRehostInstallation = {
  ...activeInstallation,
  id: "00000000-0000-4000-8000-000000000103",
  device_label: "Caja Reemplazo",
  identity_status: "unenrolled",
  identity_generation: null,
  binding_active: false,
  fingerprint: null,
};
const legacyInstallation = {
  ...activeInstallation,
  id: "00000000-0000-4000-8000-000000000104",
  device_label: "Caja Legacy",
  product: null,
  administrative_status: "unregistered",
  identity_status: "unenrolled",
  identity_generation: null,
  binding_active: false,
  fingerprint: null,
  enrolled_at: null,
  last_known_activity_at: null,
};

const activeDetail = {
  summary: activeInstallation,
  current_identity_created_at: "2026-01-02T00:00:00Z",
  current_binding: {
    bound_at: "2026-01-02T00:00:00Z",
    approved_by_account_id: "00000000-0000-4000-8000-000000000030",
  },
  rehost: null,
};
const postRehostDetail = {
  summary: postRehostInstallation,
  current_identity_created_at: null,
  current_binding: null,
  rehost: {
    id: "00000000-0000-4000-8000-000000000500",
    enrollment_request_id: "00000000-0000-4000-8000-000000000600",
    status: "pending",
    reason: "Reemplazo de equipo",
    created_at: "2026-01-11T00:00:00Z",
    expires_at: "2026-01-11T00:10:00Z",
  },
};

const baseCapabilities = ["products.read", "installations.read"];
const manageCapabilities = [...baseCapabilities, "installations.manage"];
const fullCapabilities = [...manageCapabilities, "installations.rehost"];

const staff = {
  account_id: "00000000-0000-4000-8000-000000000001",
  display_name: "Control Admin",
  email: "admin@hiqnet.test",
  role: "platform_admin",
};

type Result = { status?: number; body?: unknown };
type Api = {
  list?: (url: URL) => Result;
  detail?: (id: string) => Result;
  mutation?: (action: string, id: string, body: Record<string, unknown>) => Result;
};

async function mockSession(page: Page, capabilities: string[] = fullCapabilities) {
  await page.route("**/api/v1/control/auth/me", (route) =>
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ ...staff, capabilities }),
    }),
  );
}

async function mockApi(page: Page, api: Api) {
  await page.route("**/api/v1/control/installations**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const segments = url.pathname.split("/").filter(Boolean);
    let result: Result;
    if (request.method() === "GET" && url.pathname === "/api/v1/control/installations") {
      result = api.list?.(url) ?? { body: { items: [], next_cursor: null } };
    } else if (request.method() === "GET") {
      result = api.detail?.(segments[4] ?? "") ?? {
        status: 404,
        body: { code: "not_found", message: "x" },
      };
    } else if (request.method() === "POST") {
      let body: Record<string, unknown> = {};
      try {
        body = (request.postDataJSON() ?? {}) as Record<string, unknown>;
      } catch {
        body = {};
      }
      result = api.mutation?.(segments[5] ?? "", segments[4] ?? "", body) ?? { status: 200 };
    } else {
      result = { status: 405 };
    }
    const body = result.body === undefined ? undefined : JSON.stringify(result.body);
    await route.fulfill({
      status: result.status ?? 200,
      contentType: "application/json",
      body,
    });
  });
}

test("lista installations con estados reales y product legacy", async ({ page }) => {
  await mockSession(page);
  await mockApi(page, {
    list: () => ({
      body: {
        items: [
          activeInstallation,
          suspendedInstallation,
          postRehostInstallation,
          retiredInstallation,
          legacyInstallation,
        ],
        next_cursor: null,
      },
    }),
  });
  await page.goto("/control/installations");
  await expect(page.getByRole("heading", { name: "Installations", exact: true })).toBeVisible();
  await expect(page.getByText("Caja Centro")).toBeVisible();
  await expect(page.locator(".badge--suspended").first()).toBeVisible();
  await expect(page.locator(".badge--unenrolled").first()).toBeVisible();
  await expect(page.locator(".badge--retired").first()).toBeVisible();
  await expect(page.getByText("Sin producto asignado", { exact: true })).toBeVisible();
  await expect(page.locator('td[data-label="Last known activity"]').first()).toBeAttached();
  await expect(page.getByText("Online", { exact: true })).toHaveCount(0);
  await expect(page.getByText("Offline", { exact: true })).toHaveCount(0);
});

test("filtros de status, identity y búsqueda resetean el cursor", async ({ page }) => {
  await mockSession(page);
  const requested: string[] = [];
  await mockApi(page, {
    list: (url) => {
      requested.push(url.search);
      const items =
        url.searchParams.get("status") === "suspended"
          ? [suspendedInstallation]
          : [activeInstallation, suspendedInstallation];
      return { body: { items, next_cursor: null } };
    },
  });
  await page.goto("/control/installations?cursor=stale");
  await page.getByLabel("Administrative status").selectOption("suspended");
  await expect(page).toHaveURL(/status=suspended/);
  await expect(page).not.toHaveURL(/cursor=/);
  await expect(page.getByText("Caja Norte")).toBeVisible();
  await expect(page.getByText("Caja Centro")).toHaveCount(0);
  await page.getByLabel("Identity status").selectOption("unenrolled");
  await expect(page).toHaveURL(/identity_status=unenrolled/);
  await page.getByLabel("Search").fill("Caja");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page).toHaveURL(/search=Caja/);
  await expect(page).not.toHaveURL(/cursor=/);
  expect(requested.some((search) => search.includes("identity_status=unenrolled"))).toBe(true);
});

test("paginación por cursor avanza sin offsets", async ({ page }) => {
  await mockSession(page);
  await mockApi(page, {
    list: (url) =>
      url.searchParams.get("cursor") === "cursor-2"
        ? { body: { items: [suspendedInstallation], next_cursor: null } }
        : { body: { items: [activeInstallation], next_cursor: "cursor-2" } },
  });
  await page.goto("/control/installations");
  await expect(page.getByText("Caja Centro")).toBeVisible();
  await page.getByRole("button", { name: "Next page" }).click();
  await expect(page).toHaveURL(/cursor=cursor-2/);
  await expect(page.getByText("Caja Norte")).toBeVisible();
  await expect(page.getByText("Caja Centro")).toHaveCount(0);
});

test("listado vacío filtrado y error de servidor", async ({ page }) => {
  await mockSession(page);
  await mockApi(page, { list: () => ({ body: { items: [], next_cursor: null } }) });
  await page.goto("/control/installations?status=active");
  await expect(page.getByText("No installations match these filters")).toBeVisible();
  await page.unroute("**/api/v1/control/installations**");
  await mockApi(page, { list: () => ({ status: 500, body: { message: "boom" } }) });
  await page.goto("/control/installations");
  await expect(page.getByRole("heading", { name: "No fue posible cargar esta vista" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Reintentar" })).toBeVisible();
});

test("detalle muestra organización, producto, binding y rehost pendiente", async ({ page }) => {
  await mockSession(page);
  await mockApi(page, { detail: () => ({ body: postRehostDetail }) });
  await page.goto(`/control/installations/${postRehostInstallation.id}`);
  await expect(page.getByRole("heading", { name: "Caja Reemplazo" })).toBeVisible();
  await expect(page.getByText("Org Alpha")).toBeVisible();
  await expect(page.getByText("HiQNet POS")).toBeVisible();
  await expect(page.getByText("Esperando validación del nuevo dispositivo")).toBeVisible();
  await expect(page.getByText("Sin binding activo.")).toBeVisible();
  await expect(page.getByText("Reemplazo de equipo")).toBeVisible();
  await expect(page.getByText("Pending", { exact: true })).toBeVisible();
  await expect(page.getByText("Error de identidad")).toHaveCount(0);
});

test("detalle 404 muestra mensaje neutral", async ({ page }) => {
  await mockSession(page);
  await mockApi(page, { detail: () => ({ status: 404, body: { code: "not_found" } }) });
  await page.goto("/control/installations/00000000-0000-4000-8000-000000000999");
  await expect(page.getByRole("heading", { name: "Installation no encontrada" })).toBeVisible();
});

test("401 en listado dispara el lifecycle central y vuelve a login", async ({ page }) => {
  await page.route("**/api/v1/control/auth/me", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ ...staff, capabilities: fullCapabilities }) }),
  );
  await page.route("**/api/v1/control/installations**", (route) =>
    route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ message: "expired" }) }),
  );
  await page.goto("/control/installations");
  await expect(page.getByRole("heading", { name: "Control Center" })).toBeVisible();
});

test("read-only consulta sin acciones de gestión", async ({ page }) => {
  await mockSession(page, baseCapabilities);
  await mockApi(page, { detail: () => ({ body: activeDetail }) });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await expect(page.getByRole("heading", { name: "Caja Centro" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Suspend" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Reactivate" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Authorize rehost" })).toHaveCount(0);
});

test("operator gestiona suspend/reactivate pero no ve rehost", async ({ page }) => {
  await mockSession(page, manageCapabilities);
  await mockApi(page, {
    detail: (id) =>
      id === suspendedInstallation.id
        ? { body: { ...activeDetail, summary: suspendedInstallation } }
        : { body: activeDetail },
  });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await expect(page.getByRole("button", { name: "Suspend" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Authorize rehost" })).toHaveCount(0);
  await page.goto(`/control/installations/${suspendedInstallation.id}`);
  await expect(page.getByRole("button", { name: "Reactivate" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Authorize rehost" })).toHaveCount(0);
});

test("suspend confirma, invalida y muestra el estado confirmado", async ({ page }) => {
  await mockSession(page);
  let suspended = false;
  await mockApi(page, {
    detail: () => ({ body: suspended ? { ...activeDetail, summary: suspendedInstallation } : activeDetail }),
    mutation: (action) => {
      if (action === "suspend") {
        suspended = true;
        return { body: { ...activeDetail, summary: suspendedInstallation } };
      }
      return { status: 409, body: { code: "transition_conflict" } };
    },
  });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await page.getByRole("button", { name: "Suspend" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("No elimina el historial");
  await dialog.getByRole("button", { name: "Suspend" }).click();
  await expect(page.getByText("Suspended", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Reactivate" })).toBeVisible();
});

test("suspend 403 y 409 muestran el error sin cerrar el diálogo", async ({ page }) => {
  await mockSession(page);
  let status = 403;
  let code = "forbidden";
  await mockApi(page, {
    detail: () => ({ body: activeDetail }),
    mutation: () => ({ status, body: { code } }),
  });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await page.getByRole("button", { name: "Suspend" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Suspend" }).click();
  await expect(dialog.getByRole("alert")).toContainText("No tienes permisos");
  status = 409;
  code = "transition_conflict";
  await dialog.getByRole("button", { name: "Suspend" }).click();
  await expect(dialog.getByRole("alert")).toContainText("transición");
  await expect(dialog).toBeVisible();
});

test("reactivate exitoso y conflicto 409", async ({ page }) => {
  await mockSession(page);
  let reactivated = false;
  let conflict = false;
  await mockApi(page, {
    detail: () => ({
      body: reactivated ? activeDetail : { ...activeDetail, summary: suspendedInstallation },
    }),
    mutation: () => {
      if (conflict) return { status: 409, body: { code: "transition_conflict" } };
      reactivated = true;
      return { body: activeDetail };
    },
  });
  await page.goto(`/control/installations/${suspendedInstallation.id}`);
  await page.getByRole("button", { name: "Reactivate" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Reactivate" }).click();
  await expect(page.getByText("Active", { exact: true }).first()).toBeVisible();
  conflict = true;
  await page.getByRole("button", { name: "Suspend" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Suspend" }).click();
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("transición");
});

test("rehost autoriza y refleja el estado post-cutover", async ({ page }) => {
  await mockSession(page);
  let rehosted = false;
  let received: Record<string, unknown> = {};
  await mockApi(page, {
    detail: () => ({ body: rehosted ? postRehostDetail : activeDetail }),
    mutation: (action, _id, body) => {
      if (action !== "rehost") return { status: 404 };
      received = body;
      rehosted = true;
      return {
        body: {
          ...postRehostDetail.rehost,
          enrollment_request_id: body.enrollment_request_id,
          reason: body.reason,
        },
      };
    },
  });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await page.getByRole("button", { name: "Authorize rehost" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("Se retirará la identidad criptográfica actual");
  await expect(dialog).toContainText("El installation_id no cambia");
  await expect(dialog).not.toContainText("se perderán ventas");
  await dialog.getByLabel("Enrollment request ID").fill("00000000-0000-4000-8000-000000000600");
  await dialog.getByLabel("Reason").fill("Reemplazo de equipo");
  await dialog.getByRole("button", { name: "Authorize rehost" }).click();
  await expect(page.getByText("Esperando validación del nuevo dispositivo")).toBeVisible();
  await expect(page.getByText("Not enrolled", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("Sin binding activo.")).toBeVisible();
  await expect(page.getByText("Pending", { exact: true })).toBeVisible();
  expect(received.enrollment_request_id).toBe("00000000-0000-4000-8000-000000000600");
  expect(received.reason).toBe("Reemplazo de equipo");
});

test("rehost traduce 403, 409 y validación", async ({ page }) => {
  await mockSession(page);
  let response: Result = { status: 403, body: { code: "forbidden" } };
  await mockApi(page, {
    detail: () => ({ body: activeDetail }),
    mutation: () => response,
  });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  const openAndSubmit = async () => {
    await page.getByRole("button", { name: "Authorize rehost" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Enrollment request ID").fill("00000000-0000-4000-8000-000000000600");
    await dialog.getByLabel("Reason").fill("reemplazo");
    await dialog.getByRole("button", { name: "Authorize rehost" }).click();
    return dialog;
  };
  let dialog = await openAndSubmit();
  await expect(dialog.getByRole("alert")).toContainText("No tienes permisos");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  response = { status: 409, body: { code: "rehost_conflict" } };
  dialog = await openAndSubmit();
  await expect(dialog.getByRole("alert")).toContainText("conflicto");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  response = { status: 409, body: { code: "rehost_expired" } };
  dialog = await openAndSubmit();
  await expect(dialog.getByRole("alert")).toContainText("expiró");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  response = { status: 409, body: { code: "product_unavailable" } };
  dialog = await openAndSubmit();
  await expect(dialog.getByRole("alert")).toContainText("producto no está disponible");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  response = { status: 400, body: { code: "validation_error" } };
  dialog = await openAndSubmit();
  await expect(dialog.getByRole("alert")).toContainText("Revisa los datos");
});

test("fingerprint truncado en UI y copia del valor completo", async ({ page }) => {
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await mockSession(page);
  await mockApi(page, { detail: () => ({ body: activeDetail }) });
  await page.goto(`/control/installations/${activeInstallation.id}`);
  await expect(page.getByText("A12F…93BC")).toBeVisible();
  await page.getByRole("button", { name: "Copiar fingerprint completo" }).click();
  await expect(page.getByText("Copied")).toBeVisible();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toBe(fingerprint);
});

test("C5 sólo consume Control API y no endpoints legacy", async ({ page }) => {
  await page.route("**/api/v1/installations**", (route) => route.abort("failed"));
  await mockSession(page);
  await mockApi(page, {
    list: () => ({ body: { items: [activeInstallation], next_cursor: null } }),
    detail: () => ({ body: activeDetail }),
  });
  await page.goto("/control/installations");
  await expect(page.getByText("Caja Centro")).toBeVisible();
  await page.getByRole("link", { name: /Caja Centro/ }).click();
  await expect(page.getByRole("heading", { name: "Caja Centro" })).toBeVisible();
});

test("sin installations.read el módulo no aparece en navegación", async ({ page }) => {
  await mockSession(page, ["products.read"]);
  await page.route("**/api/v1/control/products**", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify({ items: [], next_cursor: null }) }),
  );
  await page.goto("/control/products");
  await expect(page.getByRole("link", { name: "Installations" })).toHaveCount(0);
});
