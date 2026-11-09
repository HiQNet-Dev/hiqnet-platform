import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api } from '../api/client';
import { apiErrorFromResponse } from '../api/errors';
import { AuthContext } from './auth-context';
import type { AuthContextValue, AuthStatus } from './auth-context';

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const meQuery = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: async () => {
      const result = await api.GET('/api/v1/auth/me');
      if (result.response.status === 401) {
        return null;
      }
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data ?? null;
    },
  });

  const loginMutation = useMutation({
    mutationFn: async ({ email, password }: { email: string; password: string }) => {
      const result = await api.POST('/api/v1/auth/login', {
        body: { email, password },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
    },
  });

  const logoutMutation = useMutation({
    mutationFn: async () => {
      const result = await api.POST('/api/v1/auth/logout');
      if (result.error && result.response.status !== 401) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
    },
  });

  const value = useMemo<AuthContextValue>(() => {
    const status: AuthStatus = meQuery.isPending
      ? 'loading'
      : meQuery.isError
        ? 'error'
        : meQuery.data
          ? 'authenticated'
          : 'anonymous';

    return {
      account: meQuery.data ?? null,
      status,
      error: meQuery.error,
      login: async (email, password) => {
        await loginMutation.mutateAsync({ email, password });
        const refreshed = await meQuery.refetch();
        if (!refreshed.data) {
          throw new Error('La sesión no se estableció correctamente.');
        }
        await queryClient.invalidateQueries({ queryKey: ['organizations'] });
      },
      logout: async () => {
        await logoutMutation.mutateAsync();
        queryClient.clear();
        queryClient.setQueryData(['auth', 'me'], null);
      },
      refetch: async () => {
        await meQuery.refetch();
      },
    };
  }, [loginMutation, logoutMutation, meQuery, queryClient]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
