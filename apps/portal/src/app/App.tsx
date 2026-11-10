import { Navigate, Route, Routes } from 'react-router-dom';

import { RequireAuth } from '../auth/RequireAuth';
import { AppShell } from '../layouts/AppShell';
import { LoginPage } from '../features/account/LoginPage';
import { AccountSecurityPage } from '../features/account/AccountSecurityPage';
import { OrganizationsPage } from '../features/organizations/OrganizationsPage';
import { OrganizationLayout } from '../features/organizations/OrganizationLayout';
import { OrganizationOverviewPage } from '../features/organizations/OrganizationOverviewPage';
import { MembersPage } from '../features/members/MembersPage';
import { EnrollmentPage } from '../features/enrollment/EnrollmentPage';
import { InstallationsPage } from '../features/installations/InstallationsPage';
import {
  ProductDetailPage as CommercialProductDetailPage,
  ProductsPage as CommercialProductsPage,
} from '../features/commercial/ProductsPage';
import { NotFoundPage } from './NotFoundPage';

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<RequireAuth />}>
        <Route element={<AppShell />}>
          <Route index element={<Navigate to="/app/organizations" replace />} />
          <Route path="/app" element={<Navigate to="/app/organizations" replace />} />
          <Route path="/app/organizations" element={<OrganizationsPage />} />
          <Route path="/app/organizations/:organizationId" element={<OrganizationLayout />}>
            <Route index element={<OrganizationOverviewPage />} />
            <Route path="members" element={<MembersPage />} />
            <Route path="installations" element={<InstallationsPage />} />
            <Route path="products" element={<CommercialProductsPage />} />
            <Route path="products/:productId" element={<CommercialProductDetailPage />} />
          </Route>
          <Route path="/enroll" element={<EnrollmentPage />} />
          <Route path="/account" element={<Navigate to="/account/security" replace />} />
          <Route path="/account/security" element={<AccountSecurityPage />} />
        </Route>
      </Route>
      <Route path="*" element={<NotFoundPage />} />
    </Routes>
  );
}
