import { AlertCircle } from 'lucide-react';

import { errorMessage } from '../api/errors';

export function ErrorBanner({ error, title }: { error: unknown; title?: string }) {
  return (
    <div className="error-banner" role="alert">
      <AlertCircle size={18} aria-hidden="true" />
      <div>
        {title && <strong>{title}</strong>}
        <p>{errorMessage(error)}</p>
      </div>
    </div>
  );
}
