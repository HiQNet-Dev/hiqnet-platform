/**
 * Identidad corporativa de HiQNet Platform.
 *
 * Usa los recursos originales de marca (logotipo completo con engranaje
 * orbital y degradado, e isotipo suelto) sin reconstruirlos ni deformarlos.
 * El logotipo horizontal mantiene su proporción 1850 × 550.
 */
export function Brand({ compact = false }: { compact?: boolean }) {
  if (compact) {
    return (
      <img
        className="brand__mark"
        src="/brand/hiqnet-mark.webp"
        alt="HiQNet"
        width={28}
        height={28}
      />
    );
  }
  return (
    <img
      className="brand__logo"
      src="/brand/hiqnet-logo.webp"
      alt="HiQNet"
      width={94}
      height={28}
    />
  );
}
