import { NavLink, Outlet, useParams } from 'react-router-dom';
import { Building2, LayoutDashboard, Users, Server, Package } from 'lucide-react';

import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { ErrorBanner } from '../../components/ErrorBanner';
import { Skeleton } from '../../components/Skeleton';
import { useOrganization } from './queries';
import type { Organization } from './queries';
import { organizationStatusLabel } from '../../utils/format';

export type OrganizationOutletContext = {
  organization: Organization;
};

function roleLabel(role: string): string {
  if (role === 'owner') return 'Propietario';
  if (role === 'admin') return 'Administrador';
  return 'Miembro';
}

export function OrganizationLayout() {
  const { organizationId } = useParams();
  const organizationQuery = useOrganization(organizationId);

  if (organizationQuery.isPending) {
    return (
      <div className="page-stack">
        <Skeleton className="skeleton--title" />
        <Skeleton className="skeleton--row" />
        <Skeleton className="skeleton--block" />
      </div>
    );
  }

  if (organizationQuery.isError) {
    return (
      <div className="page-stack">
        <ErrorBanner error={organizationQuery.error} title="No pudimos abrir esta organización" />
        <Button variant="secondary" onClick={() => void organizationQuery.refetch()}>
          Reintentar
        </Button>
      </div>
    );
  }

  const organization = organizationQuery.data;
  if (!organization) {
    return null;
  }

  return (
    <div className="page-stack">
      <header className="organization-header">
        <div className="organization-header__identity">
          <span className="organization-icon" aria-hidden="true">
            <Building2 size={22} />
          </span>
          <div>
            <p className="eyebrow">Organización</p>
            <h1>{organization.display_name}</h1>
            <div className="organization-header__meta">
              <Badge tone={organization.role === 'owner' ? 'accent' : 'success'}>
                {roleLabel(organization.role)}
              </Badge>
              <span className="status-label">
                <span className="status-dot" aria-hidden="true" />
                {organizationStatusLabel(organization.status)}
              </span>
            </div>
          </div>
        </div>
      </header>

      <nav className="section-tabs" aria-label="Secciones de la organización">
        <NavLink
          to={`/app/organizations/${organization.id}`}
          end
          className={({ isActive }) => `section-tabs__link${isActive ? ' is-active' : ''}`}
        >
          <LayoutDashboard size={16} aria-hidden="true" />
          Resumen
        </NavLink>
        <NavLink
          to={`/app/organizations/${organization.id}/members`}
          className={({ isActive }) => `section-tabs__link${isActive ? ' is-active' : ''}`}
        >
          <Users size={16} aria-hidden="true" />
          Miembros
        </NavLink>
        <NavLink
          to={`/app/organizations/${organization.id}/installations`}
          className={({ isActive }) => `section-tabs__link${isActive ? ' is-active' : ''}`}
        >
          <Server size={16} aria-hidden="true" />
          Instalaciones
        </NavLink>
        <NavLink
          to={`/app/organizations/${organization.id}/products`}
          className={({ isActive }) => `section-tabs__link${isActive ? ' is-active' : ''}`}
        >
          <Package size={16} aria-hidden="true" />
          Productos
        </NavLink>
      </nav>

      <Outlet context={{ organization } satisfies OrganizationOutletContext} />
    </div>
  );
}
