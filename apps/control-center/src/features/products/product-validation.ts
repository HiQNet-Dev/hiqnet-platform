import { ControlApiError } from "../../api/control-client";

/** Clave técnica estable: minúsculas ASCII, dígitos y guiones; 1–64 caracteres. */
export const PRODUCT_KEY_PATTERN = /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/;
export const PRODUCT_KEY_MAX = 64;
export const PRODUCT_NAME_MAX = 200;
export const PRODUCT_DESCRIPTION_MAX = 2000;

export type ProductDraft = {
  key?: string;
  name: string;
  description: string;
};

/**
 * Validación de cliente alineada con `platform-core::product`. No sustituye al
 * backend: sólo evita enviar solicitudes que se rechazarían con 400.
 */
export function validateProductDraft({ key, name, description }: ProductDraft): string | undefined {
  if (key !== undefined) {
    if (!key) return "La clave es obligatoria.";
    if (key.length > PRODUCT_KEY_MAX) return `La clave no puede superar ${PRODUCT_KEY_MAX} caracteres.`;
    if (!PRODUCT_KEY_PATTERN.test(key)) {
      return "La clave debe usar minúsculas, dígitos y guiones, empezando y terminando en letra o número.";
    }
  }
  const trimmedName = name.trim();
  if (!trimmedName) return "El nombre es obligatorio.";
  if (trimmedName.length > PRODUCT_NAME_MAX) {
    return `El nombre no puede superar ${PRODUCT_NAME_MAX} caracteres.`;
  }
  if (description.length > PRODUCT_DESCRIPTION_MAX) {
    return `La descripción no puede superar ${PRODUCT_DESCRIPTION_MAX} caracteres.`;
  }
  return undefined;
}

/** Mensajes estables para mutaciones de producto; nunca exponen detalles internos. */
export function productErrorCopy(error: unknown): string {
  if (error instanceof ControlApiError) {
    switch (error.status) {
      case 400:
        return "Revisa los datos: la clave, el nombre o el estado no son válidos.";
      case 403:
        return "Tu rol no permite modificar productos.";
      case 404:
        return "El producto ya no existe.";
      case 409:
        return "Ya existe un producto con esa clave. Usa una clave distinta.";
      default:
        break;
    }
  }
  return "No fue posible guardar el producto. Inténtalo de nuevo.";
}

/** Indica si el cambio de estado requiere confirmación explícita. */
export function isSensitiveStatusChange(previous: string, next: string): boolean {
  return previous !== next;
}
