import type { ReactNode } from 'react';

export function PageHeader({
  eyebrow,
  title,
  description,
  actions,
  level = 1,
}: {
  eyebrow?: string;
  title: string;
  description?: string;
  actions?: ReactNode;
  /** `2` para secciones dentro de una organización, que ya tienen su propio h1. */
  level?: 1 | 2;
}) {
  const Heading = level === 2 ? 'h2' : 'h1';
  return (
    <header className="page-header">
      <div>
        {eyebrow && <p className="page-header__eyebrow">{eyebrow}</p>}
        <Heading>{title}</Heading>
        {description && <p className="page-header__description">{description}</p>}
      </div>
      {actions && <div className="page-header__actions">{actions}</div>}
    </header>
  );
}
