import { FormEvent, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ControlApiError,
  controlClient,
  type ControlInstallationDetail,
} from "../../api/control-client";
import { useCapability } from "../../auth/session";
import { ErrorState, LoadingState } from "../../components/states";
import { ControlDialog } from "../../components/dialog";
import { formatDateTime } from "../../lib/format-date";
import {
  administrativeStatusLabel,
  fingerprintShort,
  identityStatusDescriptions,
  identityStatusLabel,
  installationErrorCopy,
  rehostStatusLabel,
} from "./installation-format";

export function InstallationDetailPage() {
  const { installationId = "" } = useParams();
  const canManage = useCapability("installations.manage");
  const canRehost = useCapability("installations.rehost");
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ["control", "installation", installationId],
    queryFn: () => controlClient.installation(installationId),
  });

  if (query.isPending) return <LoadingState label="Cargando installation…" />;
  if (query.isError) {
    const status = query.error instanceof ControlApiError ? query.error.status : undefined;
    if (status === 404) {
      return (
        <section className="page">
          <ErrorState title="Installation no encontrada" />
        </section>
      );
    }
    if (status === 403) {
      return (
        <section className="page">
          <ErrorState title="Acceso insuficiente" />
        </section>
      );
    }
    return (
      <section className="page">
        <ErrorState retry={() => void query.refetch()} />
      </section>
    );
  }

  return (
    <InstallationDetailContent
      detail={query.data}
      canManage={canManage}
      canRehost={canRehost}
      refresh={async () => {
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: ["control", "installation", installationId] }),
          queryClient.invalidateQueries({ queryKey: ["control", "installations"] }),
        ]);
      }}
    />
  );
}

type DialogKind = "suspend" | "reactivate" | "rehost";

function InstallationDetailContent({
  detail,
  canManage,
  canRehost,
  refresh,
}: {
  detail: ControlInstallationDetail;
  canManage: boolean;
  canRehost: boolean;
  refresh: () => Promise<void>;
}) {
  const installation = detail.summary;
  const [dialog, setDialog] = useState<DialogKind>();
  const [fingerprintCopied, setFingerprintCopied] = useState(false);
  const suspend = useMutation({
    mutationFn: () => controlClient.suspendInstallation(installation.id),
    onSuccess: async () => {
      setDialog(undefined);
      await refresh();
    },
  });
  const reactivate = useMutation({
    mutationFn: () => controlClient.reactivateInstallation(installation.id),
    onSuccess: async () => {
      setDialog(undefined);
      await refresh();
    },
  });
  const rehost = useMutation({
    mutationFn: (body: { enrollment_request_id: string; reason: string }) =>
      controlClient.authorizeInstallationRehost(installation.id, body),
    onSuccess: async () => {
      setDialog(undefined);
      await refresh();
    },
  });

  const open = (kind: DialogKind) => {
    suspend.reset();
    reactivate.reset();
    rehost.reset();
    setDialog(kind);
  };

  const administrative = installation.administrative_status;
  const identity = installation.identity_status;
  const rehostPending = detail.rehost?.status === "pending";
  const canTransitionAdministrative = administrative === "active" || administrative === "suspended";
  const identityAllowsRehost = identity === "active" || (identity === "unenrolled" && !rehostPending);
  const showSuspend = canManage && administrative === "active";
  const showReactivate = canManage && administrative === "suspended";
  const showRehost = canRehost && canTransitionAdministrative && identityAllowsRehost;
  const hasActions = showSuspend || showReactivate || showRehost;

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/installations" className="back-link">
            Installations
          </Link>
          <p className="eyebrow">Installation</p>
          <h1>{installation.device_label}</h1>
          <p>
            <code>{installation.id}</code>
          </p>
        </div>
        <div className="badges">
          <span className={`badge badge--${administrative}`}>
            {administrativeStatusLabel(administrative)}
          </span>
          <span className={`badge badge--${identity}`}>{identityStatusLabel(identity)}</span>
        </div>
      </div>

      {hasActions && (
        <div className="actions" aria-label="Acciones de installation">
          {showSuspend && (
            <button className="button button--danger" onClick={() => open("suspend")}>
              Suspend
            </button>
          )}
          {showReactivate && (
            <button className="button" onClick={() => open("reactivate")}>
              Reactivate
            </button>
          )}
          {showRehost && (
            <button className="button button--danger" onClick={() => open("rehost")}>
              {identity === "active" ? "Authorize rehost" : "Re-authorize rehost"}
            </button>
          )}
        </div>
      )}

      {identity === "unenrolled" && rehostPending && (
        <section className="notice" role="status">
          <h2>Esperando validación del nuevo dispositivo</h2>
          <p>
            La identidad anterior fue retirada y el binding revocado. La Installation completará el
            rehost cuando el nuevo dispositivo presente un proof válido para el enrollment request
            autorizado.
          </p>
        </section>
      )}

      <section className="panel">
        <h2>Identity</h2>
        <p>{identityStatusDescriptions[identity]}</p>
        {identity === "active" ? (
          <dl className="details details--inline">
            <div>
              <dt>Current identity generation</dt>
              <dd>{installation.identity_generation ?? "—"}</dd>
            </div>
            <div>
              <dt>Fingerprint</dt>
              <dd>
                <Fingerprint
                  value={installation.fingerprint}
                  copied={fingerprintCopied}
                  onCopied={() => setFingerprintCopied(true)}
                />
              </dd>
            </div>
            <div>
              <dt>Current identity created</dt>
              <dd>{formatDateTime(detail.current_identity_created_at)}</dd>
            </div>
            <div>
              <dt>Binding</dt>
              <dd>{installation.binding_active ? "Active" : "None"}</dd>
            </div>
          </dl>
        ) : (
          <p className="muted">
            {identity === "unenrolled"
              ? "No hay identidad criptográfica activa ni binding vigente."
              : "La identidad criptográfica de esta Installation fue retirada de forma terminal."}
          </p>
        )}
      </section>

      <section className="panel">
        <h2>Organization</h2>
        <dl className="details details--inline">
          <div>
            <dt>Organization</dt>
            <dd>{installation.organization.display_name}</dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>{installation.organization.status}</dd>
          </div>
          <div>
            <dt>UUID</dt>
            <dd>
              <code>{installation.organization.id}</code>
            </dd>
          </div>
        </dl>
      </section>

      <section className="panel">
        <h2>Product</h2>
        {installation.product ? (
          <dl className="details details--inline">
            <div>
              <dt>Product</dt>
              <dd>{installation.product.name}</dd>
            </div>
            <div>
              <dt>Key</dt>
              <dd>
                <code>{installation.product.key}</code>
              </dd>
            </div>
            <div>
              <dt>UUID</dt>
              <dd>
                <code>{installation.product.id}</code>
              </dd>
            </div>
          </dl>
        ) : (
          <p className="muted">Sin producto asignado (installation legacy pendiente de backfill).</p>
        )}
      </section>

      <section className="panel">
        <h2>Timestamps</h2>
        <dl className="details details--inline">
          <div>
            <dt>Registered</dt>
            <dd>{formatDateTime(installation.registered_at)}</dd>
          </div>
          <div>
            <dt>Enrolled</dt>
            <dd>{formatDateTime(installation.enrolled_at)}</dd>
          </div>
          <div>
            <dt>Last known activity</dt>
            <dd>{formatDateTime(installation.last_known_activity_at)}</dd>
          </div>
        </dl>
      </section>

      <section className="panel">
        <h2>Current binding</h2>
        {detail.current_binding ? (
          <dl className="details details--inline">
            <div>
              <dt>Bound at</dt>
              <dd>{formatDateTime(detail.current_binding.bound_at)}</dd>
            </div>
            <div>
              <dt>Approved by account</dt>
              <dd>
                <code>{detail.current_binding.approved_by_account_id}</code>
              </dd>
            </div>
          </dl>
        ) : (
          <p className="muted">Sin binding activo.</p>
        )}
      </section>

      {detail.rehost && (
        <section className="panel">
          <h2>Rehost authorization</h2>
          <dl className="details details--inline">
            <div>
              <dt>Status</dt>
              <dd>
                <span className={`badge badge--${detail.rehost.status}`}>
                  {rehostStatusLabel(detail.rehost.status)}
                </span>
              </dd>
            </div>
            <div>
              <dt>Enrollment request</dt>
              <dd>
                <code>{detail.rehost.enrollment_request_id}</code>
              </dd>
            </div>
            <div>
              <dt>Reason</dt>
              <dd>{detail.rehost.reason}</dd>
            </div>
            <div>
              <dt>Created</dt>
              <dd>{formatDateTime(detail.rehost.created_at)}</dd>
            </div>
            <div>
              <dt>Expires</dt>
              <dd>{formatDateTime(detail.rehost.expires_at)}</dd>
            </div>
          </dl>
        </section>
      )}

      {dialog === "suspend" && (
        <ControlDialog title="Suspend installation" onClose={() => setDialog(undefined)}>
          <p>
            Esta acción suspende administrativamente la Installation. No elimina el historial en
            Platform, no borra la identidad criptográfica ni revoca el binding.
          </p>
          <MutationError error={suspend.error} />
          <div className="dialog__actions">
            <button className="button" onClick={() => setDialog(undefined)}>
              Cancel
            </button>
            <button
              className="button button--danger"
              disabled={suspend.isPending}
              onClick={() => suspend.mutate()}
            >
              {suspend.isPending ? "Suspending…" : "Suspend"}
            </button>
          </div>
        </ControlDialog>
      )}

      {dialog === "reactivate" && (
        <ControlDialog title="Reactivate installation" onClose={() => setDialog(undefined)}>
          <p>
            Esta acción reactiva administrativamente la Installation. No modifica la identidad
            criptográfica ni el binding actuales.
          </p>
          <MutationError error={reactivate.error} />
          <div className="dialog__actions">
            <button className="button" onClick={() => setDialog(undefined)}>
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={reactivate.isPending}
              onClick={() => reactivate.mutate()}
            >
              {reactivate.isPending ? "Reactivating…" : "Reactivate"}
            </button>
          </div>
        </ControlDialog>
      )}

      {dialog === "rehost" && (
        <ControlDialog title="Authorize rehost" onClose={() => setDialog(undefined)}>
          <form
            onSubmit={(event: FormEvent<HTMLFormElement>) => {
              event.preventDefault();
              const data = new FormData(event.currentTarget);
              const enrollmentRequest = String(data.get("enrollment_request_id") ?? "").trim();
              const reason = String(data.get("reason") ?? "").trim();
              if (!enrollmentRequest || !reason) return;
              rehost.mutate({ enrollment_request_id: enrollmentRequest, reason });
            }}
          >
            <p>
              El owner comercial ya debe haber aprobado el enrollment request indicado. Esta
              pantalla no crea ni aprueba enrollments.
            </p>
            <ul className="dialog__warning">
              <li>Se retirará la identidad criptográfica actual.</li>
              <li>Se revocará el binding activo.</li>
              <li>La Installation quedará esperando el proof del nuevo dispositivo.</li>
              <li>El installation_id no cambia.</li>
              <li>El estado administrativo actual (active o suspended) se conserva.</li>
            </ul>
            <label>
              Enrollment request ID
              <input name="enrollment_request_id" required autoComplete="off" />
            </label>
            <label>
              Reason
              <textarea name="reason" required maxLength={500} rows={3} />
            </label>
            <MutationError error={rehost.error} />
            <div className="dialog__actions">
              <button className="button" type="button" onClick={() => setDialog(undefined)}>
                Cancel
              </button>
              <button className="button button--danger" disabled={rehost.isPending}>
                {rehost.isPending ? "Authorizing…" : "Authorize rehost"}
              </button>
            </div>
          </form>
        </ControlDialog>
      )}
    </section>
  );
}

function MutationError({ error }: { error: unknown }) {
  return error ? (
    <p className="inline-error" role="alert">
      {installationErrorCopy(error)}
    </p>
  ) : null;
}

async function copyToClipboard(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }
  const area = document.createElement("textarea");
  area.value = value;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  document.execCommand("copy");
  document.body.removeChild(area);
}

function Fingerprint({
  value,
  copied,
  onCopied,
}: {
  value?: string | null;
  copied: boolean;
  onCopied: () => void;
}) {
  if (!value) return <span>—</span>;
  return (
    <span className="fingerprint">
      <code title={value}>{fingerprintShort(value)}</code>
      <button
        className="button button--ghost"
        type="button"
        onClick={async () => {
          await copyToClipboard(value);
          onCopied();
        }}
        aria-label="Copiar fingerprint completo"
      >
        Copy
      </button>
      {copied && (
        <span className="muted" role="status">
          Copied
        </span>
      )}
    </span>
  );
}
