import { Navigate, Route, Routes } from "react-router-dom";
import { useSession } from "../auth/session";
import { ErrorState, LoadingState } from "../components/states";
import { OrganizationsPage } from "../features/organizations/organizations-page";
import { ProductDetailPage, ProductsPage } from "../features/products/products-pages";
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
export function AppRouter() { return <Routes><Route path="/control/login" element={<LoginRoute />} /><Route path="/control" element={<ProtectedRoute />}><Route index element={<Navigate to="products" replace />} /><Route path="products" element={<ProductsPage />} /><Route path="products/:productKey" element={<ProductDetailPage />} /><Route path="organizations" element={<OrganizationsPage />} /><Route path="forbidden" element={<AccessDenied />} /></Route><Route path="*" element={<Navigate to="/control" replace />} /></Routes>; }
