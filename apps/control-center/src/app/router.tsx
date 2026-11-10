import { Navigate, Route, Routes } from "react-router-dom";
import { useSession } from "../auth/session";
import { ErrorState, LoadingState } from "../components/states";
import { AccountDetailPage, AccountsPage } from "../features/accounts/accounts-page";
import { InstallationDetailPage } from "../features/installations/installation-detail-page";
import { InstallationsPage } from "../features/installations/installations-page";
import { OrganizationDetailPage, OrganizationsPage } from "../features/organizations/organizations-page";
import { ProductDetailPage, ProductsPage } from "../features/products/products-pages";
import { PlanDetailPage, PlansPage } from "../features/commercial/plans-page";
import {
  SubscriptionDetailPage,
  SubscriptionsPage,
} from "../features/commercial/subscriptions-page";
import { AuditDetailPage, AuditPage } from "../features/audit/audit-page";
import {
  SecurityEventDetailPage,
  SecurityEventsPage,
  StaffAuthEventDetailPage,
  StaffAuthEventsPage,
} from "../features/audit/security-page";
import { AppShell } from "../layouts/app-shell";
import { LoginPage } from "./login-page";

function ProtectedRoute() {
  const session = useSession();
  if (session.kind === "loading") return <LoadingState label="Comprobando sesión…" />;
  if (session.kind === "network-error") return <ErrorState title="No se pudo comprobar la sesión" retry={session.retry} />;
  if (session.kind === "anonymous") return <Navigate to="/control/login" replace />;
  return <AppShell />;
}
function LoginRoute() {
  const session = useSession();
  if (session.kind === "loading") return <LoadingState label="Comprobando sesión…" />;
  if (session.kind === "network-error") return <ErrorState title="No se pudo comprobar la sesión" retry={session.retry} />;
  return session.kind === "authenticated" ? <Navigate to="/control/products" replace /> : <LoginPage />;
}
function AccessDenied() { return <section className="page"><ErrorState title="Acceso insuficiente" /></section>; }
export function AppRouter() {
  return (
    <Routes>
      <Route path="/control/login" element={<LoginRoute />} />
      <Route path="/control" element={<ProtectedRoute />}>
        <Route index element={<Navigate to="products" replace />} />
        <Route path="products" element={<ProductsPage />} />
        <Route path="products/:productKey" element={<ProductDetailPage />} />
        <Route path="plans" element={<PlansPage />} />
        <Route path="plans/:planId" element={<PlanDetailPage />} />
        <Route path="subscriptions" element={<SubscriptionsPage />} />
        <Route path="subscriptions/:subscriptionId" element={<SubscriptionDetailPage />} />
        <Route path="audit" element={<AuditPage />} />
        <Route path="audit/:eventId" element={<AuditDetailPage />} />
        <Route path="security" element={<SecurityEventsPage />} />
        <Route path="security/staff-auth" element={<StaffAuthEventsPage />} />
        <Route path="security/staff-auth/:eventId" element={<StaffAuthEventDetailPage />} />
        <Route path="security/:eventId" element={<SecurityEventDetailPage />} />
        <Route path="organizations" element={<OrganizationsPage />} />
        <Route path="organizations/:organizationId" element={<OrganizationDetailPage />} />
        <Route path="accounts" element={<AccountsPage />} />
        <Route path="accounts/:accountId" element={<AccountDetailPage />} />
        <Route path="installations" element={<InstallationsPage />} />
        <Route path="installations/:installationId" element={<InstallationDetailPage />} />
        <Route path="forbidden" element={<AccessDenied />} />
      </Route>
      <Route path="*" element={<Navigate to="/control" replace />} />
    </Routes>
  );
}
