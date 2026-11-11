import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { controlClient } from "../api/control-client";
import { useRefreshSession, useSession } from "../auth/session";
import { Brand } from "../components/brand";
import { controlModules } from "../modules/registry";

function initials(displayName: string) {
  return displayName
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

export function AppShell() {
  const session = useSession();
  const navigate = useNavigate();
  const location = useLocation();
  const refresh = useRefreshSession();
  const logout = useMutation({
    mutationFn: controlClient.logout,
    onSettled: async () => {
      await refresh();
      navigate("/control/login", { replace: true });
    },
  });
  if (session.kind !== "authenticated") return null;
  const crumbs = location.pathname.split("/").filter(Boolean).slice(1);
  const navigation = controlModules.filter(
    (module) => !module.requiredCapability || session.staff.capabilities.includes(module.requiredCapability),
  );
  return (
    <div className="shell">
      <a className="skip-link" href="#main-content">
        Saltar al contenido
      </a>
      <aside className="sidebar">
        <Link className="brand" to="/control/products" aria-label="HiQNet Control Center">
          <Brand />
        </Link>
        <nav aria-label="Control Center navigation">
          {navigation.map((module) => (
            <NavLink
              key={module.id}
              to={module.route}
              className={({ isActive }) => `nav-link ${isActive ? "is-active" : ""}`}
            >
              {module.label}
            </NavLink>
          ))}
        </nav>
        <p className="environment">Internal · Platform</p>
      </aside>
      <main className="shell__main" id="main-content">
        <header className="topbar">
          <p className="breadcrumbs">Control / {crumbs.join(" / ") || "Products"}</p>
          <div className="user-menu">
            <span className="user-menu__avatar" aria-hidden="true">
              {initials(session.staff.display_name)}
            </span>
            <span>{session.staff.display_name}</span>
            <button
              className="button button--ghost"
              onClick={() => logout.mutate()}
              disabled={logout.isPending}
            >
              Log out
            </button>
          </div>
        </header>
        <Outlet />
      </main>
    </div>
  );
}
