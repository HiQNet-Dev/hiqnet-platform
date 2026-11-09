import { BrandMark } from './BrandMark';

export function FullPageLoading({ label = 'Cargando…' }: { label?: string }) {
  return (
    <div className="full-page-state" role="status" aria-live="polite">
      <BrandMark />
      <span className="spinner" aria-hidden="true" />
      <p>{label}</p>
    </div>
  );
}
