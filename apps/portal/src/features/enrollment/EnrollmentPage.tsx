import { useMemo, useState } from 'react';
import type { FormEvent } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  KeyRound,
  Link2,
  ShieldCheck,
} from 'lucide-react';

import { errorMessage } from '../../api/errors';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { FormField, Input, Select } from '../../components/FormField';
import { PageHeader } from '../../components/PageHeader';
import { useToast } from '../../components/useToast';
import { useOrganizations } from '../organizations/queries';
import { formatUserCode, isValidUserCode } from './user-code';
import { useApproveEnrollment } from './hooks';
import { classifyEnrollmentError } from './errors';
import type { EnrollmentPhase as Phase } from './errors';

export function EnrollmentPage() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const toast = useToast();
  const organizations = useOrganizations();
  const approve = useApproveEnrollment();

  const initialCode = formatUserCode(searchParams.get('code') ?? '');
  const [code, setCode] = useState(initialCode);
  const [phase, setPhase] = useState<Phase>('idle');
  const [codeError, setCodeError] = useState<string | null>(null);
  const [selectedOrganization, setSelectedOrganization] = useState('');
  const [approvedOrganizationName, setApprovedOrganizationName] = useState('');
  const [failure, setFailure] = useState<unknown>(null);

  const ownerOrganizations = useMemo(
    () =>
      (organizations.data?.pages.flatMap((page) => page.items) ?? []).filter(
        (organization) => organization.role === 'owner' && organization.status === 'active',
      ),
    [organizations.data],
  );

  async function handleContinue(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    if (!isValidUserCode(code)) {
      setCodeError('Introduce un código de ocho caracteres, por ejemplo ABCD-EFGH.');
      return;
    }
    setCodeError(null);
    setPhase('validating');
    const refreshed = await organizations.refetch();
    const owners =
      refreshed.data?.pages
        .flatMap((page) => page.items)
        .filter(
          (organization) => organization.role === 'owner' && organization.status === 'active',
        ) ?? [];
    if (owners.length === 0) {
      setPhase('forbidden');
      return;
    }
    setSelectedOrganization(owners[0].id);
    setPhase('confirm');
  }

  async function handleApprove() {
    if (!selectedOrganization) return;
    setPhase('submitting');
    setFailure(null);
    try {
      const result = await approve.mutateAsync({
        userCode: code,
        organizationId: selectedOrganization,
      });
      const organizationName =
        ownerOrganizations.find((organization) => organization.id === result.organization_id)
          ?.display_name ?? 'tu organización';
      setApprovedOrganizationName(organizationName);
      setCode('');
      setPhase('success');
      toast.push('Enrolamiento aprobado.', 'success');
      navigate('/enroll', { replace: true });
    } catch (error) {
      setFailure(error);
      setPhase(classifyEnrollmentError(error));
    }
  }

  function reset() {
    setPhase('idle');
    setFailure(null);
    setApprovedOrganizationName('');
    setSelectedOrganization('');
    navigate('/enroll', { replace: true });
  }

  const selectedOrganizationName =
    ownerOrganizations.find((organization) => organization.id === selectedOrganization)
      ?.display_name ?? '';

  return (
    <div className="page-stack">
      <PageHeader
        eyebrow="Enrolamiento"
        title="Vincular una instalación"
        description="Aprueba desde el Portal el código que muestra HiQNet POS. La prueba criptográfica la completa el dispositivo, nunca el navegador."
      />

      <div className="enrollment-layout">
        <section className="panel enrollment-main" aria-live="polite">
          {phase === 'idle' || phase === 'validating' ? (
            <>
              <div className="panel__heading">
                <div>
                  <p className="eyebrow">Paso 1</p>
                  <h2>Código de enrolamiento</h2>
                </div>
                <KeyRound size={20} aria-hidden="true" />
              </div>
              <p className="panel__intro">
                Abre HiQNet POS en el equipo que quieres vincular y copia el código de ocho
                caracteres que aparece en pantalla.
              </p>
              <form className="form-stack" onSubmit={(event) => void handleContinue(event)}>
                <FormField
                  label="Código de usuario"
                  hint="Formato ABCD-EFGH. No distingue mayúsculas de minúsculas."
                  error={codeError ?? undefined}
                >
                  {({ id, describedBy }) => (
                    <Input
                      id={id}
                      aria-describedby={describedBy}
                      className="input--code"
                      value={code}
                      onChange={(event) => setCode(formatUserCode(event.target.value))}
                      placeholder="ABCD-EFGH"
                      autoComplete="off"
                      spellCheck={false}
                      autoFocus
                      maxLength={9}
                    />
                  )}
                </FormField>
                <Button
                  type="submit"
                  loading={phase === 'validating' || organizations.isFetching}
                  disabled={!isValidUserCode(code)}
                >
                  Continuar
                  <ArrowRight size={16} />
                </Button>
              </form>
            </>
          ) : phase === 'confirm' || phase === 'submitting' ? (
            <>
              <div className="panel__heading">
                <div>
                  <p className="eyebrow">Paso 2</p>
                  <h2>Confirmar vinculación</h2>
                </div>
                <Link2 size={20} aria-hidden="true" />
              </div>
              <p className="panel__intro">
                Selecciona la organización propietaria. Sólo los propietarios pueden aprobar
                enrolamientos.
              </p>
              <dl className="confirm-list">
                <div>
                  <dt>Código</dt>
                  <dd>
                    <code className="mono-code">{code}</code>
                  </dd>
                </div>
                <div>
                  <dt>Organización</dt>
                  <dd>
                    <Select
                      aria-label="Organización propietaria"
                      value={selectedOrganization}
                      disabled={phase === 'submitting'}
                      onChange={(event) => setSelectedOrganization(event.target.value)}
                    >
                      {ownerOrganizations.map((organization) => (
                        <option key={organization.id} value={organization.id}>
                          {organization.display_name}
                        </option>
                      ))}
                    </Select>
                  </dd>
                </div>
              </dl>
              <div className="security-note">
                <ShieldCheck size={18} aria-hidden="true" />
                <p>
                  Vincula únicamente dispositivos de tu negocio que reconozcas. El código no
                  demuestra por sí solo la identidad física del equipo.
                </p>
              </div>
              <div className="button-row">
                <Button variant="ghost" onClick={reset} disabled={phase === 'submitting'}>
                  Cambiar código
                </Button>
                <Button
                  loading={phase === 'submitting'}
                  onClick={() => void handleApprove()}
                  disabled={!selectedOrganization}
                >
                  Aprobar instalación
                </Button>
              </div>
            </>
          ) : phase === 'success' ? (
            <div className="state-panel state-panel--success">
              <span className="state-icon state-icon--success" aria-hidden="true">
                <CheckCircle2 size={24} />
              </span>
              <h2>Instalación aprobada</h2>
              <p>
                {approvedOrganizationName} podrá completar la vinculación. Regresa a HiQNet POS; el
                dispositivo firmará la prueba automáticamente.
              </p>
              <div className="state-panel__actions">
                <Link className="button button--secondary button--md" to="/app/organizations">
                  Volver a organizaciones
                </Link>
                <Button onClick={reset}>Aprobar otro dispositivo</Button>
              </div>
            </div>
          ) : (
            <div className="state-panel">
              <span className="state-icon state-icon--danger" aria-hidden="true">
                <AlertTriangle size={24} />
              </span>
              <h2>{stateTitle(phase)}</h2>
              <p>{stateDescription(phase, failure)}</p>
              <div className="state-panel__actions">
                {phase === 'not_found' || phase === 'expired' ? (
                  <Button onClick={reset}>Probar otro código</Button>
                ) : (
                  <Button onClick={() => void handleApprove()} disabled={!selectedOrganization}>
                    Reintentar
                  </Button>
                )}
                {phase === 'forbidden' && (
                  <Link className="button button--secondary" to="/app/organizations">
                    Ver mis organizaciones
                  </Link>
                )}
              </div>
            </div>
          )}
        </section>

        <aside className="panel enrollment-aside">
          <p className="eyebrow">Cómo funciona</p>
          <h2>El navegador aprueba. El POS demuestra posesión.</h2>
          <ol className="process-list">
            <li>
              <span>1</span>
              <p>El POS genera una clave Ed25519 y solicita un challenge a Platform.</p>
            </li>
            <li>
              <span>2</span>
              <p>El owner introduce el código y elige la organización correcta.</p>
            </li>
            <li>
              <span>3</span>
              <p>El POS firma el nonce y completa la vinculación. El navegador no ve esa clave.</p>
            </li>
          </ol>
          {selectedOrganizationName && phase === 'submitting' && (
            <Badge tone="accent">{selectedOrganizationName}</Badge>
          )}
          <p className="product-gap">
            <ShieldCheck size={15} aria-hidden="true" />
            La API actual no expone metadata pública previa del challenge; el Portal no inventa
            nombre, plataforma ni versión del dispositivo.
          </p>
        </aside>
      </div>
    </div>
  );
}

function stateTitle(phase: Phase): string {
  switch (phase) {
    case 'expired':
      return 'El código expiró';
    case 'not_found':
      return 'Código no encontrado';
    case 'forbidden':
      return 'No puedes aprobar este enrolamiento';
    case 'rate_limited':
      return 'Demasiados intentos';
    case 'conflict':
      return 'El enrolamiento ya no es válido';
    default:
      return 'No fue posible aprobar el enrolamiento';
  }
}

function stateDescription(phase: Phase, failure: unknown): string {
  switch (phase) {
    case 'expired':
      return 'Genera un código nuevo desde HiQNet POS. El Portal no puede renovarlo.';
    case 'not_found':
      return 'Revisa el código. No encontramos una solicitud de enrolamiento activa con esos datos.';
    case 'forbidden':
      return 'La organización debe tener un propietario activo y el actor debe ser ese propietario.';
    case 'rate_limited':
      return 'Espera un momento e inténtalo nuevamente.';
    case 'conflict':
      return 'La aprobación dejó de ser válida o la solicitud entró en conflicto. Genera un flujo nuevo desde el POS.';
    default:
      return errorMessage(failure);
  }
}
