import { describe, expect, it } from "vitest";
import { ControlApiError } from "../../api/control-client";
import {
  administrativeStatusLabel,
  fingerprintShort,
  identityStatusLabel,
  installationErrorCopy,
  rehostStatusLabel,
} from "./installation-format";

describe("etiquetas de estado de installation", () => {
  it("distingue administrative status sin confundirlo con conectividad", () => {
    expect(administrativeStatusLabel("active")).toBe("Active");
    expect(administrativeStatusLabel("suspended")).toBe("Suspended");
    expect(administrativeStatusLabel("unregistered")).toContain("legacy");
    expect(administrativeStatusLabel(undefined)).toBe("—");
  });

  it("describe identity status separado del estado administrativo", () => {
    expect(identityStatusLabel("active")).toBe("Identity active");
    expect(identityStatusLabel("unenrolled")).toBe("Not enrolled");
    expect(identityStatusLabel("retired")).toBe("Identity retired");
  });

  it("etiqueta el estado de la autorización de rehost", () => {
    expect(rehostStatusLabel("pending")).toBe("Pending");
    expect(rehostStatusLabel("consumed")).toBe("Consumed");
    expect(rehostStatusLabel("expired")).toBe("Expired");
    expect(rehostStatusLabel("cancelled")).toBe("Cancelled");
  });
});

describe("presentación de fingerprint", () => {
  it("trunca sólo para mostrar y conserva valores cortos", () => {
    const full = "a12f0000000000000000000000000000000000000000000000000000000093bc";
    expect(fingerprintShort(full)).toBe("A12F…93BC");
    expect(fingerprintShort("abc123")).toBe("ABC123");
    expect(fingerprintShort(null)).toBe("—");
  });
});

describe("copy de errores de mutación", () => {
  it("traduce los códigos 409 sin inventar semántica", () => {
    expect(installationErrorCopy(new ControlApiError(409, "x", "transition_conflict"))).toContain(
      "transición",
    );
    expect(installationErrorCopy(new ControlApiError(409, "x", "rehost_conflict"))).toContain(
      "conflicto",
    );
    expect(installationErrorCopy(new ControlApiError(409, "x", "rehost_expired"))).toContain(
      "expiró",
    );
    expect(installationErrorCopy(new ControlApiError(409, "x", "product_unavailable"))).toContain(
      "producto",
    );
  });

  it("cubre permisos, validación y fallback", () => {
    expect(installationErrorCopy(new ControlApiError(403, "x", "forbidden"))).toContain(
      "permisos",
    );
    expect(installationErrorCopy(new ControlApiError(400, "x", "validation_error"))).toContain(
      "Revisa",
    );
    expect(installationErrorCopy(new Error("boom"))).toBe(
      "No fue posible completar la operación.",
    );
  });
});
