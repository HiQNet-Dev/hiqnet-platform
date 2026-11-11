import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { ArrowRight, Building2, Plus } from 'lucide-react';

import { errorMessage } from '../../api/errors';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { EmptyState } from '../../components/EmptyState';
import { ErrorBanner } from '../../components/ErrorBanner';
import { FormField, Input } from '../../components/FormField';
import { PageHeader } from '../../components/PageHeader';
import { Skeleton } from '../../components/Skeleton';
import { useToast } from '../../components/useToast';
import { useCreateOrganization, useOrganizations } from './queries';
import type { Organization } from './queries';

function roleTone(role: string): 'accent' | 'success' | 'neutral' {
  if (role === 'owner') return 'accent';
  if (role === 'admin') return 'success';
  return 'neutral';
}

function roleLabel(role: string): string {
  if (role === 'owner') return 'Propietario';
  if (role === 'admin') return 'Administrador';
  return 'Miembro';
}

function OrganizationRows({ items }: { items: Organization[] }) {
  return (
    <div className="table-wrap">
      <table className="data-table">
        <thead>
          <tr>
            <th scope="col">Organización</th>
            <th scope="col">Tu rol</th>
            <th scope="col">Identificador</th>
            <th scope="col" className="data-table__action">
              <span className="sr-only">Abrir</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {items.map((organization) => (
            <tr key={organization.id}>
              <td>
                <Link className="table-primary" to={`/app/organizations/${organization.id}`}>
                  <span className="table-primary__icon" aria-hidden="true">
                    <Building2 size={17} />
                  </span>
                  {organization.display_name}
                </Link>
              </td>
              <td>
                <Badge tone={roleTone(organization.role)}>{roleLabel(organization.role)}</Badge>
              </td>
              <td>
                <code className="mono-id">{organization.id.slice(0, 8)}</code>
              </td>
              <td className="data-table__action">
                <Link
                  className="icon-button"
                  to={`/app/organizations/${organization.id}`}
                  aria-label={`Abrir ${organization.display_name}`}
                >
                  <ArrowRight size={17} />
                </Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function OrganizationsPage() {
  const organizations = useOrganizations();
  const createOrganization = useCreateOrganization();
  const toast = useToast();
  const navigate = useNavigate();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [name, setName] = useState('');

  const items = organizations.data?.pages.flatMap((page) => page.items) ?? [];

  async function handleCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      return;
    }
    try {
      const created = await createOrganization.mutateAsync(trimmed);
      setDialogOpen(false);
      setName('');
      toast.push('Organización creada.', 'success');
      if (created?.id) {
        navigate(`/app/organizations/${created.id}`);
      }
    } catch {
      // El error se muestra dentro del diálogo.
    }
  }

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="Espacio comercial"
        title="Organizaciones"
        description="Selecciona el negocio que quieres administrar. Tus permisos se verifican en Platform en cada operación."
        actions={
          <Button icon={<Plus size={17} />} onClick={() => setDialogOpen(true)}>
            Nueva organización
          </Button>
        }
      />

      {/* Un fallo de API no debe presentarse además como "no hay organizaciones". */}
      {organizations.isError ? (
        <ErrorBanner error={organizations.error} />
      ) : organizations.isPending ? (
        <div className="panel">
          <Skeleton className="skeleton--row" />
          <Skeleton className="skeleton--row" />
          <Skeleton className="skeleton--row" />
        </div>
      ) : items.length === 0 ? (
        <EmptyState
          icon={<Building2 size={22} />}
          title="Todavía no tienes organizaciones"
          description="Crea la primera organización para gestionar miembros e instalaciones HiQNet."
          action={
            <Button variant="secondary" onClick={() => setDialogOpen(true)}>
              Crear organización
            </Button>
          }
        />
      ) : (
        <div className="panel panel--flush">
          <OrganizationRows items={items} />
          {organizations.hasNextPage && (
            <div className="table-footer">
              <Button
                variant="secondary"
                size="sm"
                loading={organizations.isFetchingNextPage}
                onClick={() => void organizations.fetchNextPage()}
              >
                Cargar más organizaciones
              </Button>
            </div>
          )}
        </div>
      )}

      <Dialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        title="Nueva organización"
        description="La organización será la autoridad comercial del negocio y agrupará a sus miembros."
        footer={
          <>
            <Button variant="ghost" onClick={() => setDialogOpen(false)}>
              Cancelar
            </Button>
            <Button
              loading={createOrganization.isPending}
              disabled={!name.trim()}
              onClick={() => void handleCreate()}
            >
              Crear organización
            </Button>
          </>
        }
      >
        <FormField label="Nombre de la organización" hint="Visible para todos sus miembros.">
          {({ id }) => (
            <Input
              id={id}
              value={name}
              maxLength={200}
              autoFocus
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  void handleCreate();
                }
              }}
            />
          )}
        </FormField>
        {createOrganization.isError && (
          <p className="inline-notice inline-notice--error" role="alert">
            {errorMessage(createOrganization.error)}
          </p>
        )}
      </Dialog>
    </div>
  );
}
