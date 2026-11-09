import { AlertTriangle } from 'lucide-react';

import { errorMessage } from '../api/errors';
import { Button } from './Button';

export function ServiceError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  return (
    <main className="full-page-state">
      <span className="state-icon state-icon--danger" aria-hidden="true">
        <AlertTriangle size={22} />
      </span>
      <h1>El servicio no está disponible</h1>
      <p>{errorMessage(error)}</p>
      <Button onClick={onRetry}>Reintentar</Button>
    </main>
  );
}
