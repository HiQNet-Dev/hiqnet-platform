const DEFAULT_AUTHENTICATED_PATH = '/app/organizations';

export function safeInternalPath(value: unknown): string {
  if (typeof value !== 'string') {
    return DEFAULT_AUTHENTICATED_PATH;
  }
  if (!value.startsWith('/') || value.startsWith('//') || value.startsWith('/login')) {
    return DEFAULT_AUTHENTICATED_PATH;
  }
  return value;
}
