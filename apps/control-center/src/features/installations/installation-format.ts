import {
  ControlApiError,
  type InstallationAdministrativeStatus,
  type InstallationIdentityStatus,
  type InstallationRehostStatus,
} from "../../api/control-client";

export const administrativeStatusLabels: Record<InstallationAdministrativeStatus, string> = {
  active: "Active",
  suspended: "Suspended",
  unregistered: "Unregistered (legacy)",
};

export const identityStatusLabels: Record<InstallationIdentityStatus, string> = {
  active: "Identity active",
  unenrolled: "Not enrolled",
  retired: "Identity retired",
};

export const identityStatusDescriptions: Record<InstallationIdentityStatus, string> = {
  active: "Existe una identidad criptográfica activa y un binding vigente.",
  unenrolled: "No hay identidad ni binding activos en este momento.",
  retired: "La identidad criptográfica fue retirada de forma terminal.",
};

export const rehostStatusLabels: Record<InstallationRehostStatus, string> = {
  pending: "Pending",
  consumed: "Consumed",
  expired: "Expired",
  cancelled: "Cancelled",
};

export function administrativeStatusLabel(
  value: InstallationAdministrativeStatus | null | undefined,
): string {
  return value ? administrativeStatusLabels[value] : "—";
}

export function identityStatusLabel(
  value: InstallationIdentityStatus | null | undefined,
): string {
  return value ? identityStatusLabels[value] : "—";
}

export function rehostStatusLabel(value: InstallationRehostStatus | null | undefined): string {
  return value ? rehostStatusLabels[value] : "—";
}

export function fingerprintShort(value: string | null | undefined): string {
  if (!value) return "—";
  const display = value.toUpperCase();
  return display.length <= 12 ? display : `${display.slice(0, 4)}…${display.slice(-4)}`;
}

export function installationErrorCopy(error: unknown): string {
  if (error instanceof ControlApiError) {
    switch (error.code) {
      case "validation_error":
        return "Revisa los datos ingresados. El motivo es obligatorio y el enrollment request debe ser válido.";
      case "invalid_path":
        return "El identificador de la instalación no es válido.";
      case "forbidden":
        return "No tienes permisos para ejecutar esta operación.";
      case "not_found":
        return "La instalación no existe o ya no está disponible.";
      case "transition_conflict":
        return "La transición de estado ya no es válida para esta instalación. Actualiza la vista.";
      case "rehost_conflict":
        return "Existe un conflicto con la autorización o el enrollment request actual.";
      case "rehost_expired":
        return "La autorización o el enrollment request ya expiró. Genera uno nuevo fuera de esta pantalla.";
      case "product_unavailable":
        return "El producto no está disponible para esta operación.";
      case "storage_error":
        return "El servicio no está disponible en este momento. Inténtalo de nuevo.";
      default:
        return error.message;
    }
  }
  return "No fue posible completar la operación.";
}
