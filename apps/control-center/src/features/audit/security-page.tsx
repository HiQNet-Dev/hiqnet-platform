import { useState } from "react";
import { Link, NavLink, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { controlClient } from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { formatDateTime } from "../../lib/format-date";

const kinds = [
  "enrollment_conflict",
  "duplicate_key",
  "tenant_change_attempt",
  "replay_rejected",
  "rehost_performed",
  "identity_revoked",
  "authorization_revoked_during_proof",
  "idempotency_conflict",
  "clone_suspected",
];

const actions = [
  "login_success",
  "login_failure",
  "logout",
  "session_revoked",
  "account_locked",
  "password_changed",
];

function toRfc3339(value: string) {
  return value ? new Date(value).toISOString() : undefined;
}

function SecurityTabs() {
  return (
    <nav className="tabs" aria-label="Datasets de seguridad">
      <NavLink to="/control/security" end className="button">
        Platform events
      </NavLink>
      <NavLink to="/control/security/staff-auth" className="button">
        Staff authentication
      </NavLink>
    </nav>
  );
}

export function SecurityEventsPage() {
  const [kind, setKind] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const query = useQuery({
    queryKey: ["control", "security-events", { kind, from, to }],
    queryFn: () =>
      controlClient.securityEvents({
        kind: kind || undefined,
        from: toRfc3339(from),
        to: toRfc3339(to),
      }),
  });

  if (query.isPending) return <LoadingState label="Cargando security events…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Security</p>
          <h1>Platform events</h1>
          <p>Eventos de seguridad comercial y de instalaciones.</p>
        </div>
      </div>
      <SecurityTabs />
      <div className="filters" aria-label="Filtros de security events">
        <label>
          Kind
          <select value={kind} onChange={(event) => setKind(event.target.value)}>
            <option value="">All kinds</option>
            {kinds.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label>
          From
          <input type="datetime-local" value={from} onChange={(event) => setFrom(event.target.value)} />
        </label>
        <label>
          To
          <input type="datetime-local" value={to} onChange={(event) => setTo(event.target.value)} />
        </label>
      </div>
      {query.data.items.length === 0 ? (
        <EmptyState title="No security events" body="No hay eventos para los filtros actuales." />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Timestamp</th>
                <th>Kind</th>
                <th>Organization</th>
                <th>Installation</th>
                <th>Account</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((event) => (
                <tr key={event.id}>
                  <td>{formatDateTime(event.occurred_at)}</td>
                  <td>
                    <Link to={`/control/security/${event.id}`} className="table-link">
                      {event.kind}
                      <code>{event.id}</code>
                    </Link>
                  </td>
                  <td>{event.organization?.display_name ?? "—"}</td>
                  <td>{event.installation?.device_label ?? "—"}</td>
                  <td>{event.account?.display_name ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function SecurityEventDetailPage() {
  const { eventId = "" } = useParams();
  const query = useQuery({
    queryKey: ["control", "security-event", eventId],
    queryFn: () => controlClient.securityEvent(eventId),
  });
  if (query.isPending) return <LoadingState label="Cargando evento…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }
  const event = query.data;
  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/security" className="back-link">
            Security
          </Link>
          <p className="eyebrow">Platform event</p>
          <h1>{event.kind}</h1>
          <p>{event.id}</p>
        </div>
      </div>
      <dl className="details">
        <div>
          <dt>Timestamp</dt>
          <dd>{formatDateTime(event.occurred_at)}</dd>
        </div>
        <div>
          <dt>Organization</dt>
          <dd>{event.organization?.display_name ?? "—"}</dd>
        </div>
        <div>
          <dt>Installation</dt>
          <dd>{event.installation?.device_label ?? "—"}</dd>
        </div>
        <div>
          <dt>Account</dt>
          <dd>{event.account?.display_name ?? "—"}</dd>
        </div>
      </dl>
      {event.account && (
        <dl className="details">
          <div>
            <dt>Email</dt>
            <dd>{event.account.email}</dd>
          </div>
        </dl>
      )}
      <section className="related">
        <div>
          <h2>Metadata saneada</h2>
          <p>Sólo claves permitidas para este kind.</p>
        </div>
        <pre>{JSON.stringify(event.metadata, null, 2)}</pre>
      </section>
    </section>
  );
}

export function StaffAuthEventsPage() {
  const [action, setAction] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const query = useQuery({
    queryKey: ["control", "staff-auth-events", { action, from, to }],
    queryFn: () =>
      controlClient.staffAuthEvents({
        action: action || undefined,
        from: toRfc3339(from),
        to: toRfc3339(to),
      }),
  });

  if (query.isPending) return <LoadingState label="Cargando staff auth…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Security</p>
          <h1>Staff authentication</h1>
          <p>Eventos de autenticación del personal interno.</p>
        </div>
      </div>
      <SecurityTabs />
      <div className="filters" aria-label="Filtros de staff auth">
        <label>
          Action
          <select value={action} onChange={(event) => setAction(event.target.value)}>
            <option value="">All actions</option>
            {actions.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label>
          From
          <input type="datetime-local" value={from} onChange={(event) => setFrom(event.target.value)} />
        </label>
        <label>
          To
          <input type="datetime-local" value={to} onChange={(event) => setTo(event.target.value)} />
        </label>
      </div>
      {query.data.items.length === 0 ? (
        <EmptyState title="No staff auth events" body="No hay eventos para los filtros actuales." />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Timestamp</th>
                <th>Staff account</th>
                <th>Action</th>
                <th>Context</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((event) => (
                <tr key={event.id}>
                  <td>{formatDateTime(event.occurred_at)}</td>
                  <td>{event.account?.display_name ?? "system"}</td>
                  <td>
                    <Link to={`/control/security/staff-auth/${event.id}`} className="table-link">
                      {event.action}
                      <code>{event.id}</code>
                    </Link>
                  </td>
                  <td>—</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function StaffAuthEventDetailPage() {
  const { eventId = "" } = useParams();
  const query = useQuery({
    queryKey: ["control", "staff-auth-event", eventId],
    queryFn: () => controlClient.staffAuthEvent(eventId),
  });
  if (query.isPending) return <LoadingState label="Cargando evento…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }
  const event = query.data;
  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/security/staff-auth" className="back-link">
            Staff authentication
          </Link>
          <p className="eyebrow">Staff auth event</p>
          <h1>{event.action}</h1>
          <p>{event.id}</p>
        </div>
      </div>
      <dl className="details">
        <div>
          <dt>Timestamp</dt>
          <dd>{formatDateTime(event.occurred_at)}</dd>
        </div>
        <div>
          <dt>Account</dt>
          <dd>{event.account?.display_name ?? "system"}</dd>
        </div>
        <div>
          <dt>Email</dt>
          <dd>{event.account?.email ?? "—"}</dd>
        </div>
        <div>
          <dt>Status</dt>
          <dd>{event.account?.status ?? "—"}</dd>
        </div>
      </dl>
      <section className="related">
        <div>
          <h2>Metadata saneada</h2>
          <p>No se exponen IP, user-agent, sesiones ni hashes.</p>
        </div>
        <pre>{JSON.stringify(event.metadata, null, 2)}</pre>
      </section>
    </section>
  );
}
