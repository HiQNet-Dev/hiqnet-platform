import type { Capability } from "../api/control-client";

export type ControlModule = {
  id: "products" | "organizations";
  label: string;
  route: string;
  requiredCapability?: Capability;
  description: string;
};

export const controlModules: readonly ControlModule[] = [
  { id: "products", label: "Products", route: "/control/products", requiredCapability: "products.read", description: "Registro de productos y proyectos" },
  { id: "organizations", label: "Organizations", route: "/control/organizations", description: "Foundation preparada para C4" },
];
