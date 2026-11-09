import { isApiError } from '../../api/errors';

export type EnrollmentPhase =
  | 'idle'
  | 'validating'
  | 'confirm'
  | 'submitting'
  | 'success'
  | 'expired'
  | 'not_found'
  | 'forbidden'
  | 'rate_limited'
  | 'conflict'
  | 'error';

export function classifyEnrollmentError(error: unknown): EnrollmentPhase {
  if (!isApiError(error)) return 'error';
  if (error.status === 410 || error.code === 'enrollment_expired') return 'expired';
  if (error.status === 404) return 'not_found';
  if (error.status === 403) return 'forbidden';
  if (error.status === 429) return 'rate_limited';
  if (error.status === 409) return 'conflict';
  return 'error';
}
