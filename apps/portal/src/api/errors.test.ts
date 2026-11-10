import { describe, expect, it } from 'vitest';

import { ApiError, errorMessage, isUnauthorized } from './errors';

describe('errorMessage', () => {
  it('traduce estados sin exponer detalles internos', () => {
    expect(errorMessage(new ApiError(404, 'not_found', 'detalle interno'))).toContain(
      'No encontramos este recurso',
    );
    expect(errorMessage(new ApiError(429, 'rate_limited', 'detalle interno'))).toContain(
      'Demasiados intentos',
    );
    expect(errorMessage(new ApiError(503, 'storage_error', 'detalle interno'))).toContain(
      'no está disponible',
    );
  });

  it('mantiene neutralidad cross-tenant en 404 y detecta 401', () => {
    const error = new ApiError(404, 'not_found', 'x');
    expect(errorMessage(error)).not.toContain('tenant');
    expect(isUnauthorized(error)).toBe(false);
    expect(isUnauthorized(new ApiError(401, 'unauthorized', 'x'))).toBe(true);
  });
});
