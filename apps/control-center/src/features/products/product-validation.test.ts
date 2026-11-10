import { describe, expect, it } from "vitest";

import { ControlApiError } from "../../api/control-client";
import {
  PRODUCT_DESCRIPTION_MAX,
  PRODUCT_NAME_MAX,
  isSensitiveStatusChange,
  productErrorCopy,
  validateProductDraft,
} from "./product-validation";

describe("validación de producto", () => {
  it("acepta claves estables válidas", () => {
    for (const key of ["pos", "demo-service", "a1", "x".repeat(64).slice(0, 64)]) {
      expect(validateProductDraft({ key, name: "Producto", description: "" })).toBeUndefined();
    }
  });

  it("rechaza claves no normalizadas o fuera de límites", () => {
    for (const key of ["", "POS", "-pos", "pos-", "pos--", "pos_1", "pos 1", "á"]) {
      expect(validateProductDraft({ key, name: "Producto", description: "" })).toBeTruthy();
    }
    expect(
      validateProductDraft({ key: "a".repeat(65), name: "Producto", description: "" }),
    ).toBeTruthy();
  });

  it("exige nombre no vacío y respeta longitudes máximas", () => {
    expect(validateProductDraft({ key: "pos", name: "   ", description: "" })).toBeTruthy();
    expect(
      validateProductDraft({ key: "pos", name: "N".repeat(PRODUCT_NAME_MAX + 1), description: "" }),
    ).toBeTruthy();
    expect(
      validateProductDraft({
        key: "pos",
        name: "N",
        description: "d".repeat(PRODUCT_DESCRIPTION_MAX + 1),
      }),
    ).toBeTruthy();
    expect(
      validateProductDraft({
        key: "pos",
        name: "N",
        description: "d".repeat(PRODUCT_DESCRIPTION_MAX),
      }),
    ).toBeUndefined();
  });

  it("traduce errores de API sin exponer detalles internos", () => {
    expect(productErrorCopy(new ControlApiError(409, "conflict", "conflict"))).toContain(
      "Ya existe un producto con esa clave",
    );
    expect(productErrorCopy(new ControlApiError(403, "forbidden"))).toContain("Tu rol");
    expect(productErrorCopy(new Error("Failed to fetch"))).toBe(
      "No fue posible guardar el producto. Inténtalo de nuevo.",
    );
  });

  it("considera sensible cualquier cambio de estado", () => {
    expect(isSensitiveStatusChange("active", "active")).toBe(false);
    expect(isSensitiveStatusChange("active", "retired")).toBe(true);
  });
});
