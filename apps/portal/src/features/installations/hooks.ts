import { useInfiniteQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import { apiErrorFromResponse } from '../../api/errors';
import type { components } from '../../api/schema';

export type Installation = components['schemas']['InstallationListItemResponse'];

export function installationKeys(organizationId: string) {
  return ['organizations', organizationId, 'installations'] as const;
}

export function useInstallations(organizationId: string) {
  return useInfiniteQuery({
    queryKey: installationKeys(organizationId),
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) => {
      const result = await api.GET('/api/v1/organizations/{id}/installations', {
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
