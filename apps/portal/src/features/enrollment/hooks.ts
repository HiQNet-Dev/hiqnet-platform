import { useMutation } from '@tanstack/react-query';

import { api } from '../../api/client';
import { apiErrorFromResponse } from '../../api/errors';
import type { components } from '../../api/schema';

export type ApprovalResult = components['schemas']['ApprovedEnrollmentResponse'];

export function useApproveEnrollment() {
  return useMutation({
    mutationFn: async ({
      userCode,
      organizationId,
    }: {
      userCode: string;
      organizationId: string;
    }) => {
      const result = await api.POST('/api/v1/enrollment-requests/{user_code}/approve', {
        params: { path: { user_code: userCode } },
        body: { organization_id: organizationId },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      return result.data as ApprovalResult;
    },
  });
}
