import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { OrganizationsPage } from "./organizations-page";

describe("OrganizationsPage", () => {
  it("declara honestamente que C4 aún no expone su API global", () => {
    render(<OrganizationsPage />);
    expect(screen.getByText("Módulo disponible en C4")).toBeInTheDocument();
    expect(screen.getByText(/no consulta ni reutiliza endpoints comerciales/i)).toBeInTheDocument();
  });
});
