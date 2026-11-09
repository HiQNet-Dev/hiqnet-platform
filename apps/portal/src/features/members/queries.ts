import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query';

import { api } from '../../api/client';
import { apiErrorFromResponse } from '../../api/errors';
import type { components } from '../../api/schema';

export type Member = components['schemas']['MemberResponse'];
export type MemberRole = 'owner' | 'admin' | 'member';

export function memberKeys(organizationId: string) {
  return ['organizations', organizationId, 'members'] as const;
}

export function useMembers(organizationId: string) {
  return useInfiniteQuery({
    queryKey: memberKeys(organizationId),
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) => {
      const result = await api.GET('/api/v1/organizations/{id}/members', {
        params: {
          path: { id: organizationId },
          query: { limit: 50, cursor: pageParam ?? undefined },
        },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data ?? { items: [], next_cursor: null };
    },
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? null,
  });
}

export function useAddMember(organizationId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ accountId, role }: { accountId: string; role: MemberRole }) => {
      const result = await api.POST('/api/v1/organizations/{id}/members', {
        params: { path: { id: organizationId } },
        body: { account_id: accountId, role },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data;
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: memberKeys(organizationId) });
    },
  });
}

export function useChangeMemberRole(organizationId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ accountId, role }: { accountId: string; role: MemberRole }) => {
      const result = await api.PATCH('/api/v1/organizations/{id}/members/{account_id}', {
        params: { path: { id: organizationId, account_id: accountId } },
        body: { role },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data;
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: memberKeys(organizationId) });
    },
  });
}

export function useRevokeMember(organizationId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (accountId: string) => {
      const result = await api.DELETE('/api/v1/organizations/{id}/members/{account_id}', {
        params: { path: { id: organizationId, account_id: accountId } },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: memberKeys(organizationId) });
    },
  });
}
