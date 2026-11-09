import type { Capability } from "../api/control-client";

export type ControlModule = {
  id: "products" | "organizations" | "accounts";
  label: string;
  route: string;
  requiredCapability?: Capability;
  description: string;
};

export const controlModules: readonly ControlModule[] = [
  { id: "products", label: "Products", route: "/control/products", requiredCapability: "products.read", description: "Registro de productos y proyectos" },
  { id: "organizations", label: "Organizations", route: "/control/organizations", requiredCapability: "organizations.read", description: "Organizaciones cliente globales" },
  { id: "accounts", label: "Accounts", route: "/control/accounts", requiredCapability: "accounts.read", description: "Cuentas comerciales globales" },
];
