import { Link, useOutletContext } from 'react-router-dom';
import { Copy, KeyRound, RefreshCw, Server } from 'lucide-react';

import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState } from '../../components/EmptyState';
import { ErrorBanner } from '../../components/ErrorBanner';
import { PageHeader } from '../../components/PageHeader';
import { Skeleton } from '../../components/Skeleton';
import { useToast } from '../../components/useToast';
import { formatDateTime, shortId } from '../../utils/format';
import type { OrganizationOutletContext } from '../organizations/OrganizationLayout';
import { useInstallations } from './hooks';
import type { Installation } from './hooks';

function installationStatus(status: string): {
  label: string;
  tone: 'success' | 'warning' | 'neutral';
} {
  if (status === 'active') return { label: 'Activa', tone: 'success' };
  if (status === 'suspended') return { label: 'Suspendida', tone: 'warning' };
  if (status === 'unregistered') return { label: 'No registrada', tone: 'neutral' };
  return { label: status, tone: 'neutral' };
}

function identityStatus(status: string): {
  label: string;
  tone: 'success' | 'warning' | 'neutral';
} {
  if (status === 'active') return { label: 'Identidad activa', tone: 'success' };
  if (status === 'retired') return { label: 'Identidad retirada', tone: 'warning' };
  if (status === 'unenrolled') return { label: 'Sin enrolar', tone: 'neutral' };
  return { label: status, tone: 'neutral' };
}

function InstallationRow({
  installation,
  onCopy,
}: {
  installation: Installation;
  onCopy: (id: string) => void;
}) {
  const status = installationStatus(installation.status);
  const identity = identityStatus(installation.identity_status);
  return (
    <tr>
      <td>
        <span className="installation-cell">
          <span className="table-primary__icon" aria-hidden="true">
            <Server size={16} />
          </span>
          <span>
            <strong>{installation.device_label}</strong>
            <small className="mono-id">
              {shortId(installation.installation_id)}
              <button
                type="button"
                className="icon-button icon-button--small"
                onClick={() => onCopy(installation.installation_id)}
                aria-label="Copiar identificador de instalación"
              >
                <Copy size={12} />
              </button>
            </small>
          </span>
        </span>
      </td>
      <td>
        <Badge tone={status.tone}>{status.label}</Badge>
      </td>
      <td>
        <Badge tone={identity.tone}>{identity.label}</Badge>
      </td>
      <td className="table-muted">{formatDateTime(installation.bound_at)}</td>
      <td className="table-muted">{formatDateTime(installation.last_seen_at)}</td>
    </tr>
  );
}

export function InstallationsPage() {
  const { organization } = useOutletContext<OrganizationOutletContext>();
  const installations = useInstallations(organization.id);
  const toast = useToast();
  const items = installations.data?.pages.flatMap((page) => page.items) ?? [];

  async function copyId(id: string) {
    try {
      await navigator.clipboard.writeText(id);
      toast.push('Identificador copiado.', 'success');
    } catch {
      toast.push('No fue posible copiar el identificador.', 'error');
    }
  }

  return (
    <div className="page-stack">
      <PageHeader
        level={2}
        eyebrow="Dispositivos"
        title="Instalaciones"
        description="Equipos HiQNet POS vinculados a esta organización. El estado se actualiza con la autoridad real de Platform."
        actions={
          <Button
            variant="secondary"
            icon={<RefreshCw size={16} />}
            loading={installations.isFetching}
            onClick={() => void installations.refetch()}
          >
            Actualizar
          </Button>
        }
      />

      {/* Un fallo de API no debe presentarse además como "aún no hay instalaciones". */}
      {installations.isError ? (
        <ErrorBanner error={installations.error} />
      ) : installations.isPending ? (
        <div className="panel">
          <Skeleton className="skeleton--row" />
          <Skeleton className="skeleton--row" />
        </div>
      ) : items.length === 0 ? (
        <EmptyState
          icon={<KeyRound size={22} />}
          title="Aún no hay instalaciones vinculadas"
          description="Abre HiQNet POS en el dispositivo que deseas vincular y utiliza su código de enrolamiento."
          action={
            <Link className="button button--primary button--md" to="/enroll">
              Tengo un código de enrolamiento
            </Link>
          }
        />
      ) : (
        <div className="panel panel--flush">
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">Dispositivo</th>
                  <th scope="col">Estado</th>
                  <th scope="col">Identidad</th>
                  <th scope="col">Vinculada</th>
                  <th scope="col">Última señal</th>
                </tr>
              </thead>
              <tbody>
                {items.map((installation) => (
                  <InstallationRow
                    key={installation.installation_id}
                    installation={installation}
                    onCopy={(id) => void copyId(id)}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {installations.hasNextPage && (
            <div className="table-footer">
              <Button
                variant="secondary"
                size="sm"
                loading={installations.isFetchingNextPage}
                onClick={() => void installations.fetchNextPage()}
              >
                Cargar más instalaciones
              </Button>
            </div>
          )}
        </div>
      )}

      <p className="product-gap">
        <Server size={15} aria-hidden="true" />
        La API no expone claves públicas, fingerprints, hashes ni eventos de seguridad de la
        instalación. El Portal no los solicita ni los infiere.
      </p>
    </div>
  );
}
