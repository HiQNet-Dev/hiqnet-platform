import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api } from '../../api/client';
import { apiErrorFromResponse } from '../../api/errors';
import type { components } from '../../api/schema';

export type Organization = components['schemas']['OrganizationResponse'];
export type OrganizationPage = components['schemas']['OrganizationListResponse'];

const ORGANIZATIONS_KEY = ['organizations'] as const;

export function useOrganizations() {
  return useInfiniteQuery({
    queryKey: ORGANIZATIONS_KEY,
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) => {
      const result = await api.GET('/api/v1/organizations', {
        params: { query: { limit: 50, cursor: pageParam ?? undefined } },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data ?? { items: [], next_cursor: null };
    },
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? null,
  });
}

export function useOrganization(organizationId: string | undefined) {
  return useQuery({
    queryKey: ['organizations', organizationId],
    enabled: Boolean(organizationId),
    queryFn: async () => {
      const result = await api.GET('/api/v1/organizations/{id}', {
        params: { path: { id: organizationId ?? '' } },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      if (!result.data) {
        throw new Error('Organización no disponible.');
      }
      return result.data;
    },
  });
}

export function useCreateOrganization() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (displayName: string) => {
      const result = await api.POST('/api/v1/organizations', {
        body: { display_name: displayName },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data;
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ORGANIZATIONS_KEY });
    },
  });
}
