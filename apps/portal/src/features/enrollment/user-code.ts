const USER_CODE_ALPHABET = /[A-Z0-9]/g;

export function normalizeUserCode(value: string): string {
  const compact = value.toUpperCase().match(USER_CODE_ALPHABET)?.join('') ?? '';
  return compact.slice(0, 8);
}

export function formatUserCode(value: string): string {
  const compact = normalizeUserCode(value);
  if (compact.length <= 4) {
    return compact;
  }
  return `${compact.slice(0, 4)}-${compact.slice(4)}`;
}

export function isValidUserCode(value: string): boolean {
  return /^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(formatUserCode(value));
}
