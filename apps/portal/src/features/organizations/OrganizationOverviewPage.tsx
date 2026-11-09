import { Link, useOutletContext } from 'react-router-dom';
import { ArrowRight, Copy, Server, Users } from 'lucide-react';

import { Badge } from '../../components/Badge';
import { useToast } from '../../components/useToast';
import type { OrganizationOutletContext } from './OrganizationLayout';
import { organizationStatusLabel } from '../../utils/format';

function roleLabel(role: string): string {
  if (role === 'owner') return 'Propietario';
  if (role === 'admin') return 'Administrador';
  return 'Miembro';
}

export function OrganizationOverviewPage() {
  const { organization } = useOutletContext<OrganizationOutletContext>();
  const toast = useToast();

  async function copyId() {
    try {
      await navigator.clipboard.writeText(organization.id);
      toast.push('Identificador copiado.', 'success');
    } catch {
      toast.push('No fue posible copiar el identificador.', 'error');
    }
  }

  return (
    <div className="overview-grid">
      <section className="panel">
        <div className="panel__heading">
          <div>
            <p className="eyebrow">Resumen</p>
            <h2>Datos de la organización</h2>
          </div>
          <Badge tone={organization.role === 'owner' ? 'accent' : 'success'}>
            {roleLabel(organization.role)}
          </Badge>
        </div>
        <dl className="detail-list">
          <div>
            <dt>Nombre visible</dt>
            <dd>{organization.display_name}</dd>
          </div>
          <div>
            <dt>Estado</dt>
            <dd>{organizationStatusLabel(organization.status)}</dd>
          </div>
          <div>
            <dt>Identificador</dt>
            <dd className="detail-list__id">
              <code>{organization.id}</code>
              <button
                type="button"
                className="icon-button icon-button--small"
                onClick={() => void copyId()}
                aria-label="Copiar identificador"
              >
                <Copy size={14} />
              </button>
            </dd>
          </div>
        </dl>
      </section>

      <section className="panel">
        <div className="panel__heading">
          <div>
            <p className="eyebrow">Administración</p>
            <h2>Qué puedes gestionar</h2>
          </div>
        </div>
        <div className="action-list">
          <Link className="action-list__item" to={`/app/organizations/${organization.id}/members`}>
            <span className="action-list__icon" aria-hidden="true">
              <Users size={18} />
            </span>
            <span>
              <strong>Miembros y permisos</strong>
              <small>Consulta quién pertenece a la organización y sus roles.</small>
            </span>
            <ArrowRight size={17} aria-hidden="true" />
          </Link>
          <Link
            className="action-list__item"
            to={`/app/organizations/${organization.id}/installations`}
          >
            <span className="action-list__icon" aria-hidden="true">
              <Server size={18} />
            </span>
            <span>
              <strong>Instalaciones vinculadas</strong>
              <small>Consulta los equipos enrolados y su identidad.</small>
            </span>
            <ArrowRight size={17} aria-hidden="true" />
          </Link>
        </div>
      </section>
    </div>
  );
}
