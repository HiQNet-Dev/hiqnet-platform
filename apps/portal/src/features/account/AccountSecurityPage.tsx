import { useState } from 'react';
import type { FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound, LogOut, ShieldAlert } from 'lucide-react';

import { api } from '../../api/client';
import { apiErrorFromResponse, errorMessage } from '../../api/errors';
import { useAuth } from '../../auth/useAuth';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { FormField, Input } from '../../components/FormField';
import { PageHeader } from '../../components/PageHeader';
import { useToast } from '../../components/useToast';

export function AccountSecurityPage() {
  const auth = useAuth();
  const toast = useToast();
  const navigate = useNavigate();
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [changing, setChanging] = useState(false);
  const [changeError, setChangeError] = useState<unknown>(null);
  const [revokeOpen, setRevokeOpen] = useState(false);
  const [revoking, setRevoking] = useState(false);
  const [revokeError, setRevokeError] = useState<unknown>(null);

  async function handlePasswordChange(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setChanging(true);
    setChangeError(null);
    try {
      const result = await api.POST('/api/v1/auth/password/change', {
        body: {
          current_password: currentPassword,
          new_password: newPassword,
        },
      });
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      setCurrentPassword('');
      setNewPassword('');
      toast.push('Contraseña actualizada y sesiones anteriores revocadas.', 'success');
      await auth.refetch();
    } catch (error) {
      setChangeError(error);
    } finally {
      setChanging(false);
    }
  }

  async function handleRevokeAll() {
    setRevoking(true);
    setRevokeError(null);
    try {
      const result = await api.POST('/api/v1/auth/sessions/revoke');
      if (result.error) {
        throw apiErrorFromResponse(result.response.status, result.error);
      }
      setRevokeOpen(false);
      await auth.logout();
      toast.push('Todas las sesiones fueron revocadas.', 'success');
      navigate('/login', { replace: true });
    } catch (error) {
      setRevokeError(error);
    } finally {
      setRevoking(false);
    }
  }

  async function handleLogout() {
    await auth.logout();
    navigate('/login', { replace: true });
  }

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="Cuenta"
        title="Cuenta y seguridad"
        description="Gestiona tu contraseña y las sesiones comerciales de HiQNet Platform."
      />

      <div className="settings-grid">
        <section className="panel">
          <div className="panel__heading">
            <div>
              <p className="eyebrow">Credenciales</p>
              <h2>Cambiar contraseña</h2>
            </div>
            <KeyRound size={20} aria-hidden="true" />
          </div>
          <p className="panel__intro">
            Al cambiarla se revocan las sesiones anteriores. El Portal no almacena tu contraseña.
          </p>
          <form className="form-stack" onSubmit={(event) => void handlePasswordChange(event)}>
            <FormField label="Contraseña actual">
              {({ id }) => (
                <Input
                  id={id}
                  type="password"
                  autoComplete="current-password"
                  value={currentPassword}
                  onChange={(event) => setCurrentPassword(event.target.value)}
                  required
                />
              )}
            </FormField>
            <FormField
              label="Nueva contraseña"
              hint="Entre 12 y 128 caracteres, con al menos una letra y un dígito."
            >
              {({ id, describedBy }) => (
                <Input
                  id={id}
                  aria-describedby={describedBy}
                  type="password"
                  autoComplete="new-password"
                  value={newPassword}
                  onChange={(event) => setNewPassword(event.target.value)}
                  required
                />
              )}
            </FormField>
            {changeError !== null && (
              <p className="inline-notice inline-notice--error" role="alert">
                {errorMessage(changeError)}
              </p>
            )}
            <Button type="submit" loading={changing}>
              Actualizar contraseña
            </Button>
          </form>
        </section>

        <section className="panel panel--danger-zone">
          <div className="panel__heading">
            <div>
              <p className="eyebrow">Sesiones</p>
              <h2>Control de acceso</h2>
            </div>
            <ShieldAlert size={20} aria-hidden="true" />
          </div>
          <p className="panel__intro">
            Cierra esta sesión o revoca todas las sesiones activas de tu cuenta comercial.
          </p>
          <div className="button-row">
            <Button
              variant="secondary"
              icon={<LogOut size={16} />}
              onClick={() => void handleLogout()}
            >
              Cerrar esta sesión
            </Button>
            <Button variant="danger" onClick={() => setRevokeOpen(true)}>
              Revocar todas las sesiones
            </Button>
          </div>
        </section>
      </div>

      <Dialog
        open={revokeOpen}
        onClose={() => setRevokeOpen(false)}
        title="Revocar todas las sesiones"
        description="Todas las sesiones comerciales de tu cuenta dejarán de ser válidas inmediatamente."
        footer={
          <>
            <Button variant="ghost" onClick={() => setRevokeOpen(false)}>
              Cancelar
            </Button>
            <Button variant="danger" loading={revoking} onClick={() => void handleRevokeAll()}>
              Revocar todas
            </Button>
          </>
        }
      >
        <p>También se cerrará esta sesión. Tendrás que iniciar sesión de nuevo.</p>
        {revokeError !== null && (
          <p className="inline-notice inline-notice--error" role="alert">
            {errorMessage(revokeError)}
          </p>
        )}
      </Dialog>
    </div>
  );
}
