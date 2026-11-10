import { useContext } from 'react';

import { ToastContext } from './toast-context';
import type { ToastContextValue } from './toast-context';

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast debe usarse dentro de ToastProvider');
  }
  return context;
}
