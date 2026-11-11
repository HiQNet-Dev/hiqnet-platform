export function BrandMark({ compact = false }: { compact?: boolean }) {
  if (compact) {
    return (
      <img
        className="brand-mark__mark"
        src="/brand/hiqnet-mark.webp"
        alt="HiQNet"
        width={36}
        height={36}
      />
    );
  }
  return (
    <img
      className="brand-mark__logo"
      src="/brand/hiqnet-logo.webp"
      alt="HiQNet"
      width={112}
      height={33}
    />
  );
}
