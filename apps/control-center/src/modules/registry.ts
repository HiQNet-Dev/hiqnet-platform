import type { Capability } from "../api/control-client";

export type ControlModule = {
  id: "products" | "plans" | "subscriptions" | "organizations" | "accounts" | "installations" | "audit" | "security";
  label: string;
  route: string;
  requiredCapability?: Capability;
  description: string;
};

export const controlModules: readonly ControlModule[] = [
  { id: "products", label: "Products", route: "/control/products", requiredCapability: "products.read", description: "Registro de productos y proyectos" },
  { id: "plans", label: "Plans", route: "/control/plans", requiredCapability: "products.read", description: "Catálogo comercial por producto" },
  { id: "subscriptions", label: "Subscriptions", route: "/control/subscriptions", requiredCapability: "subscriptions.read", description: "Contratos organización–producto–plan" },
  { id: "installations", label: "Installations", route: "/control/installations", requiredCapability: "installations.read", description: "Instalaciones POS y control administrativo" },
  { id: "audit", label: "Audit", route: "/control/audit", requiredCapability: "audit.read", description: "Auditoría de mutaciones staff" },
  { id: "security", label: "Security", route: "/control/security", requiredCapability: "security.read", description: "Eventos de seguridad y autenticación staff" },
  { id: "organizations", label: "Organizations", route: "/control/organizations", requiredCapability: "organizations.read", description: "Organizaciones cliente globales" },
  { id: "accounts", label: "Accounts", route: "/control/accounts", requiredCapability: "accounts.read", description: "Cuentas comerciales globales" },
];
