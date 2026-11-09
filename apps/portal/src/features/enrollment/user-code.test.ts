import { describe, expect, it } from 'vitest';

import { formatUserCode, isValidUserCode, normalizeUserCode } from './user-code';

describe('user code', () => {
  it('normaliza minúsculas, espacios y separadores', () => {
    expect(normalizeUserCode(' abcd efgh ')).toBe('ABCDEFGH');
    expect(formatUserCode('abcdefgh')).toBe('ABCD-EFGH');
    expect(formatUserCode('ABCD EFGH')).toBe('ABCD-EFGH');
    expect(formatUserCode('abcd-efgh')).toBe('ABCD-EFGH');
  });

  it('rechaza longitudes incompletas o caracteres ignorados', () => {
    expect(isValidUserCode('ABCD-EFG')).toBe(false);
    expect(isValidUserCode('ABCD-EFGH')).toBe(true);
    expect(normalizeUserCode('!!')).toBe('');
  });
});
