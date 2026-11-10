export function formatDateTime(value: string | null | undefined): string {
  if (!value) {
    return '—';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return '—';
  }
  return new Intl.DateTimeFormat('es-ES', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date);
}

export function shortId(value: string): string {
  return value.slice(0, 8);
}

export function organizationStatusLabel(value: string): string {
  if (value === 'active') return 'Activa';
  if (value === 'archived') return 'Archivada';
  if (value === 'legacy_unclaimed') return 'Sin reclamar';
  return value;
}
