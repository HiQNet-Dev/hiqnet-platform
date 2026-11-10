import { MutationCache, QueryCache, QueryClient } from '@tanstack/react-query';

import { isUnauthorized } from '../api/errors';

function redirectToLogin() {
  if (window.location.pathname.startsWith('/login')) {
    return;
  }
  const from = `${window.location.pathname}${window.location.search}`;
  window.location.assign(`/login?expired=1&from=${encodeURIComponent(from)}`);
}

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => !isUnauthorized(error) && failureCount < 1,
      refetchOnWindowFocus: false,
      staleTime: 15_000,
    },
  },
  queryCache: new QueryCache({
    onError: (error) => {
      if (isUnauthorized(error)) {
        queryClient.clear();
        redirectToLogin();
      }
    },
  }),
  mutationCache: new MutationCache({
    onError: (error) => {
      if (isUnauthorized(error)) {
        queryClient.clear();
        redirectToLogin();
      }
    },
  }),
});
