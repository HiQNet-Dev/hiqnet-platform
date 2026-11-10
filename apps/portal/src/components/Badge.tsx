import type { ReactNode } from 'react';

import { cn } from '../utils/cn';

export function Badge({
  children,
  tone = 'neutral',
}: {
  children: ReactNode;
  tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
}) {
  return <span className={cn('badge', `badge--${tone}`)}>{children}</span>;
}
