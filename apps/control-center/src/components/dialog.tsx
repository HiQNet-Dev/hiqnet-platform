import { ReactNode, useEffect, useId, useRef } from "react";

/**
 * Diálogo modal nativo con el mismo tratamiento visual en todo el Control Center.
 * `showModal` aporta foco atrapado, `Escape` y semántica `role="dialog"` sin
 * dependencias externas.
 */
export function ControlDialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <h2 id={titleId}>{title}</h2>
      {children}
    </dialog>
  );
}
