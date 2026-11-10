export function BrandMark({ compact = false }: { compact?: boolean }) {
  return (
    <span className={`brand-mark${compact ? ' brand-mark--compact' : ''}`} aria-label="HiQNet">
      <svg viewBox="0 0 24 24" aria-hidden="true" className="brand-mark__icon">
        <rect width="24" height="24" rx="5.4" fill="currentColor" />
        <g fill="#ffffff">
          <rect x="5.5" y="4.5" width="4" height="15" />
          <rect x="14.5" y="4.5" width="4" height="15" />
          <rect x="9.5" y="10" width="5" height="4" />
        </g>
      </svg>
      {!compact && (
        <span className="brand-mark__text">
          HiQNet <span>Platform</span>
        </span>
      )}
    </span>
  );
}
