import { Navigate, Outlet, useLocation } from 'react-router-dom';

import { useAuth } from './useAuth';
import { FullPageLoading } from '../components/FullPageLoading';
import { ServiceError } from '../components/ServiceError';

export function RequireAuth() {
  const auth = useAuth();
  const location = useLocation();

  if (auth.status === 'loading') {
    return <FullPageLoading label="Comprobando tu sesión…" />;
  }

  if (auth.status === 'error') {
    return <ServiceError error={auth.error} onRetry={() => void auth.refetch()} />;
  }

  if (auth.status === 'anonymous') {
    return (
      <Navigate to="/login" replace state={{ from: `${location.pathname}${location.search}` }} />
    );
  }

  return <Outlet />;
}
