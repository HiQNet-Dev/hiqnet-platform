import { useState } from 'react';
import type { FormEvent } from 'react';
import { Navigate, useLocation, useNavigate, useSearchParams } from 'react-router-dom';

import { useAuth } from '../../auth/useAuth';
import { safeInternalPath } from '../../auth/safe-redirect';
import { BrandMark } from '../../components/BrandMark';
import { Button } from '../../components/Button';
import { FormField, Input } from '../../components/FormField';
import { errorMessage } from '../../api/errors';

export function LoginPage() {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const stateFrom =
    typeof location.state === 'object' && location.state !== null && 'from' in location.state
      ? (location.state as { from?: unknown }).from
      : undefined;
  const target = safeInternalPath(searchParams.get('from') ?? stateFrom);
  const expired = searchParams.get('expired') === '1';

  if (auth.status === 'authenticated') {
    return <Navigate to={target} replace />;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await auth.login(email.trim(), password);
      navigate(target, { replace: true });
    } catch (failure) {
      setError(failure);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className="login-layout">
      <section className="login-brand" aria-label="HiQNet Platform">
        <BrandMark />
        <div className="login-brand__message">
          <p className="eyebrow">Control comercial</p>
          <h1>Tu operación HiQNet, gestionada desde un solo lugar.</h1>
          <p>
            Administra organizaciones, miembros e instalaciones vinculadas. El POS sigue siendo
            local-first; Platform sólo ejerce como autoridad comercial.
          </p>
        </div>
        <p className="login-brand__footnote">HiQNet Platform · Portal de clientes</p>
      </section>

      <section className="login-panel">
        <div className="login-form-wrap">
          <div className="login-form-wrap__header">
            <p className="eyebrow">Acceso comercial</p>
            <h2>Inicia sesión</h2>
            <p>Usa la cuenta comercial asociada a tu negocio.</p>
          </div>

          {expired && (
            <p className="inline-notice inline-notice--warning" role="status">
              Tu sesión ha caducado. Vuelve a iniciar sesión.
            </p>
          )}

          <form onSubmit={(event) => void handleSubmit(event)} className="form-stack" noValidate>
            <FormField label="Correo electrónico">
              {({ id, describedBy }) => (
                <Input
                  id={id}
                  aria-describedby={describedBy}
                  type="email"
                  name="email"
                  autoComplete="email"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  required
                  autoFocus
                />
              )}
            </FormField>
            <FormField label="Contraseña">
              {({ id, describedBy }) => (
                <Input
                  id={id}
                  aria-describedby={describedBy}
                  type="password"
                  name="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  required
                />
              )}
            </FormField>

            {error !== null && (
              <p className="inline-notice inline-notice--error" role="alert">
                {errorMessage(error)}
              </p>
            )}

            <Button type="submit" loading={submitting} className="button--block">
              Entrar
            </Button>
          </form>

          <p className="login-form-wrap__help">
            ¿Problemas para acceder? Contacta con el administrador de tu cuenta HiQNet.
          </p>
        </div>
      </section>
    </main>
  );
}
