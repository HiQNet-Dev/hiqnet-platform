import { useState } from 'react';
import { useOutletContext } from 'react-router-dom';
import { Copy, Plus, ShieldCheck, UserMinus, Users } from 'lucide-react';

import { isApiError } from '../../api/errors';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { EmptyState } from '../../components/EmptyState';
import { ErrorBanner } from '../../components/ErrorBanner';
import { FormField, Input, Select } from '../../components/FormField';
import { PageHeader } from '../../components/PageHeader';
import { Skeleton } from '../../components/Skeleton';
import { useToast } from '../../components/useToast';
import { useAuth } from '../../auth/useAuth';
import type { OrganizationOutletContext } from '../organizations/OrganizationLayout';
import { formatDateTime, shortId } from '../../utils/format';
import { useAddMember, useChangeMemberRole, useMembers, useRevokeMember } from './queries';
import type { Member, MemberRole } from './queries';

function roleLabel(role: string): string {
  if (role === 'owner') return 'Propietario';
  if (role === 'admin') return 'Administrador';
  return 'Miembro';
}

function roleTone(role: string): 'accent' | 'success' | 'neutral' {
  if (role === 'owner') return 'accent';
  if (role === 'admin') return 'success';
  return 'neutral';
}

function conflictMessage(error: unknown): string {
  if (isApiError(error) && error.status === 409) {
    return 'La organización debe conservar al menos un propietario activo.';
  }
  return error instanceof Error ? error.message : 'No fue posible completar la operación.';
}

export function MembersPage() {
  const { organization } = useOutletContext<OrganizationOutletContext>();
  const auth = useAuth();
  const membersQuery = useMembers(organization.id);
  const addMember = useAddMember(organization.id);
  const changeRole = useChangeMemberRole(organization.id);
  const revokeMember = useRevokeMember(organization.id);
  const toast = useToast();

  const isOwner = organization.role === 'owner';
  const [addOpen, setAddOpen] = useState(false);
  const [accountId, setAccountId] = useState('');
  const [newRole, setNewRole] = useState<MemberRole>('member');
  const [revokeTarget, setRevokeTarget] = useState<Member | null>(null);

  const members = membersQuery.data?.pages.flatMap((page) => page.items) ?? [];

  async function handleAdd() {
    try {
      await addMember.mutateAsync({ accountId: accountId.trim(), role: newRole });
      setAddOpen(false);
      setAccountId('');
      setNewRole('member');
      toast.push('Miembro añadido.', 'success');
    } catch {
      // El diálogo muestra el error.
    }
  }

  async function handleRoleChange(member: Member, role: MemberRole) {
    try {
      await changeRole.mutateAsync({ accountId: member.account_id, role });
      toast.push(`Rol actualizado a ${roleLabel(role).toLowerCase()}.`, 'success');
    } catch (error) {
      toast.push(conflictMessage(error), 'error');
    }
  }

  async function handleRevoke() {
    if (!revokeTarget) return;
    try {
      await revokeMember.mutateAsync(revokeTarget.account_id);
      setRevokeTarget(null);
      toast.push('Membresía revocada.', 'success');
    } catch (error) {
      toast.push(conflictMessage(error), 'error');
    }
  }

  async function copyAccountId(account: string) {
    try {
      await navigator.clipboard.writeText(account);
      toast.push('Identificador de cuenta copiado.', 'success');
    } catch {
      toast.push('No fue posible copiar el identificador.', 'error');
    }
  }

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="Acceso"
        title="Miembros"
        description={
          isOwner
            ? 'Administra quién puede acceder a esta organización. Los cambios se aplican en el backend de inmediato.'
            : 'Consulta las cuentas con acceso a esta organización. Sólo un propietario puede modificarlas.'
        }
        actions={
          isOwner ? (
            <Button icon={<Plus size={17} />} onClick={() => setAddOpen(true)}>
              Añadir miembro
            </Button>
          ) : undefined
        }
      />

      {membersQuery.isError && <ErrorBanner error={membersQuery.error} />}

      {membersQuery.isPending ? (
        <div className="panel">
          <Skeleton className="skeleton--row" />
          <Skeleton className="skeleton--row" />
        </div>
      ) : members.length === 0 ? (
        <EmptyState
          icon={<Users size={22} />}
          title="No hay miembros visibles"
          description="La organización no tiene membresías activas que mostrar."
        />
      ) : (
        <div className="panel panel--flush">
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">Cuenta</th>
                  <th scope="col">Rol</th>
                  <th scope="col">Alta</th>
                  {isOwner && (
                    <th scope="col" className="data-table__action">
                      <span className="sr-only">Acciones</span>
                    </th>
                  )}
                </tr>
              </thead>
              <tbody>
                {members.map((member) => {
                  const isSelf = member.account_id === auth.account?.account_id;
                  const busy = changeRole.isPending || revokeMember.isPending;
                  return (
                    <tr key={member.account_id}>
                      <td>
                        <span className="member-cell">
                          <span className="avatar avatar--small" aria-hidden="true">
                            {member.account_id.slice(0, 1).toUpperCase()}
                          </span>
                          <span>
                            <code className="mono-id">{shortId(member.account_id)}</code>
                            <small>{member.account_id}</small>
                          </span>
                          <button
                            type="button"
                            className="icon-button icon-button--small"
                            onClick={() => void copyAccountId(member.account_id)}
                            aria-label="Copiar identificador de cuenta"
                          >
                            <Copy size={13} />
                          </button>
                        </span>
                      </td>
                      <td>
                        {isOwner ? (
                          <Select
                            aria-label={`Rol de ${shortId(member.account_id)}`}
                            value={member.role}
                            disabled={busy || isSelf}
                            onChange={(event) =>
                              void handleRoleChange(member, event.target.value as MemberRole)
                            }
                          >
                            <option value="owner">Propietario</option>
                            <option value="admin">Administrador</option>
                            <option value="member">Miembro</option>
                          </Select>
                        ) : (
                          <Badge tone={roleTone(member.role)}>{roleLabel(member.role)}</Badge>
                        )}
                      </td>
                      <td className="table-muted">{formatDateTime(member.joined_at)}</td>
                      {isOwner && (
                        <td className="data-table__action">
                          <button
                            type="button"
                            className="icon-button icon-button--danger"
                            aria-label={`Revocar acceso de ${shortId(member.account_id)}`}
                            disabled={busy}
                            onClick={() => setRevokeTarget(member)}
                          >
                            <UserMinus size={17} />
                          </button>
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {membersQuery.hasNextPage && (
            <div className="table-footer">
              <Button
                variant="secondary"
                size="sm"
                loading={membersQuery.isFetchingNextPage}
                onClick={() => void membersQuery.fetchNextPage()}
              >
                Cargar más miembros
              </Button>
            </div>
          )}
        </div>
      )}

      <p className="product-gap">
        <ShieldCheck size={15} aria-hidden="true" />
        La API de miembros sólo expone el identificador de cuenta. La invitación por correo o
        búsqueda de personas queda como mejora de backend pendiente.
      </p>

      <Dialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
        title="Añadir miembro"
        description="Introduce el identificador de una cuenta comercial existente."
        footer={
          <>
            <Button variant="ghost" onClick={() => setAddOpen(false)}>
              Cancelar
            </Button>
            <Button
              loading={addMember.isPending}
              disabled={!accountId.trim()}
              onClick={() => void handleAdd()}
            >
              Añadir miembro
            </Button>
          </>
        }
      >
        <FormField
          label="Identificador de cuenta"
          hint="UUID de la cuenta comercial que ya existe en Platform."
        >
          {({ id }) => (
            <Input
              id={id}
              value={accountId}
              autoFocus
              onChange={(event) => setAccountId(event.target.value)}
              placeholder="00000000-0000-0000-0000-000000000000"
            />
          )}
        </FormField>
        <FormField label="Rol">
          {({ id }) => (
            <Select
              id={id}
              value={newRole}
              onChange={(event) => setNewRole(event.target.value as MemberRole)}
            >
              <option value="member">Miembro</option>
              <option value="admin">Administrador</option>
              <option value="owner">Propietario</option>
            </Select>
          )}
        </FormField>
        {addMember.isError && (
          <p className="inline-notice inline-notice--error" role="alert">
            {conflictMessage(addMember.error)}
          </p>
        )}
      </Dialog>

      <Dialog
        open={revokeTarget !== null}
        onClose={() => setRevokeTarget(null)}
        title="Revocar membresía"
        description="La cuenta perderá acceso a esta organización en la siguiente solicitud."
        footer={
          <>
            <Button variant="ghost" onClick={() => setRevokeTarget(null)}>
              Cancelar
            </Button>
            <Button
              variant="danger"
              loading={revokeMember.isPending}
              onClick={() => void handleRevoke()}
            >
              Revocar acceso
            </Button>
          </>
        }
      >
        <p>
          Vas a revocar el acceso de <code>{revokeTarget?.account_id}</code>. Esta acción no se
          puede deshacer desde el Portal.
        </p>
        {revokeMember.isError && (
          <p className="inline-notice inline-notice--error" role="alert">
            {conflictMessage(revokeMember.error)}
          </p>
        )}
      </Dialog>
    </div>
  );
}
