import { Link } from 'react-router-dom';

import { BrandMark } from '../components/BrandMark';

export function NotFoundPage() {
  return (
    <main className="full-page-state">
      <BrandMark />
      <p className="eyebrow">Error 404</p>
      <h1>No encontramos esta página</h1>
      <p>El enlace puede haber cambiado o no estar disponible para tu cuenta.</p>
      <Link className="button button--primary" to="/app/organizations">
        Volver al Portal
      </Link>
    </main>
  );
}
