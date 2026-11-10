import { describe, expect, it } from 'vitest';

import { safeInternalPath } from './safe-redirect';

describe('safeInternalPath', () => {
  it('acepta rutas internas', () => {
    expect(safeInternalPath('/app/organizations/abc')).toBe('/app/organizations/abc');
  });

  it('rechaza open redirects y vuelve al destino por defecto', () => {
    expect(safeInternalPath('https://evil.example')).toBe('/app/organizations');
    expect(safeInternalPath('//evil.example')).toBe('/app/organizations');
    expect(safeInternalPath('/login')).toBe('/app/organizations');
    expect(safeInternalPath(undefined)).toBe('/app/organizations');
  });
});
