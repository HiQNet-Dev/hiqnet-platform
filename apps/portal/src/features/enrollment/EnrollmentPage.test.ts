import { describe, expect, it } from 'vitest';

import { ApiError } from '../../api/errors';
import { classifyEnrollmentError } from './errors';

describe('classifyEnrollmentError', () => {
  it('mapea estados de enrollment a fases UX explícitas', () => {
    expect(classifyEnrollmentError(new ApiError(410, 'enrollment_expired', 'x'))).toBe('expired');
    expect(classifyEnrollmentError(new ApiError(404, 'not_found', 'x'))).toBe('not_found');
    expect(classifyEnrollmentError(new ApiError(403, 'forbidden', 'x'))).toBe('forbidden');
    expect(classifyEnrollmentError(new ApiError(429, 'rate_limited', 'x'))).toBe('rate_limited');
    expect(classifyEnrollmentError(new ApiError(409, 'enrollment_invalid', 'x'))).toBe('conflict');
    expect(classifyEnrollmentError(new Error('boom'))).toBe('error');
  });
});
