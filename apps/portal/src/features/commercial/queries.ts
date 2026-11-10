import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import { apiErrorFromResponse } from '../../api/errors';
import type { components } from '../../api/schema';

export type PortalProduct = components['schemas']['PortalProductResponse'];

export function useOrganizationProducts(organizationId: string | undefined) {
  return useQuery({
    queryKey: ['organization', organizationId, 'products'],
    enabled: Boolean(organizationId),
    queryFn: async () => {
      const result = await api.GET('/api/v1/organizations/{organization_id}/products', {
        params: { path: { organization_id: organizationId ?? '' } },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data ?? { items: [] };
    },
  });
}

export function useOrganizationProduct(
  organizationId: string | undefined,
  productId: string | undefined,
) {
  return useQuery({
    queryKey: ['organization', organizationId, 'products', productId],
    enabled: Boolean(organizationId && productId),
    queryFn: async () => {
      const result = await api.GET(
        '/api/v1/organizations/{organization_id}/products/{product_id}',
        {
          params: {
            path: {
              organization_id: organizationId ?? '',
              product_id: productId ?? '',
            },
          },
        },
      );
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      if (!result.data) {
        throw new Error('Producto no disponible.');
      }
      return result.data;
    },
  });
}
