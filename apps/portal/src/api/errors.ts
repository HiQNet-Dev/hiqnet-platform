import type { components } from './schema';

type ErrorBody = components['schemas']['ErrorResponse'];

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

export function apiErrorFromResponse(status: number, payload: unknown): ApiError {
  if (
    typeof payload === 'object' &&
    payload !== null &&
    'code' in payload &&
    typeof payload.code === 'string'
  ) {
    const body = payload as ErrorBody;
    return new ApiError(status, body.code, body.message);
  }
  return new ApiError(status, 'unknown', 'No fue posible completar la operación.');
}

const FALLBACK_MESSAGES: Record<number, string> = {
  400: 'Revisa los datos introducidos e inténtalo de nuevo.',
  401: 'Tu sesión ha caducado. Inicia sesión de nuevo.',
  403: 'No tienes permisos para realizar esta operación.',
  404: 'No encontramos este recurso o no está disponible para tu cuenta.',
  409: 'La operación entra en conflicto con el estado actual.',
  410: 'Este recurso ya no está disponible.',
  429: 'Demasiados intentos. Espera un momento antes de reintentar.',
  503: 'El servicio no está disponible temporalmente. Inténtalo de nuevo.',
};

export function errorMessage(error: unknown): string {
  if (!isApiError(error)) {
    return 'No fue posible completar la operación. Inténtalo de nuevo.';
  }
  return FALLBACK_MESSAGES[error.status] ?? error.message;
}

export function isUnauthorized(error: unknown): boolean {
  return isApiError(error) && error.status === 401;
}
