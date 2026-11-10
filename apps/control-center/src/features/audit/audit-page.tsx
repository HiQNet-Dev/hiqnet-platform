import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { controlClient, type ControlAuditEvent } from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { formatDateTime } from "../../lib/format-date";

const results = ["success", "denied", "failure"];

function toRfc3339(value: string) {
  return value ? new Date(value).toISOString() : undefined;
}

export function AuditPage() {
  const [action, setAction] = useState("");
  const [result, setResult] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const query = useQuery({
    queryKey: ["control", "audit", { action, result, from, to }],
    queryFn: () =>
      controlClient.audit({
        action: action.trim() || undefined,
        result: result || undefined,
        from: toRfc3339(from),
        to: toRfc3339(to),
      }),
  });

  if (query.isPending) return <LoadingState label="Cargando audit…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Security</p>
          <h1>Audit</h1>
          <p>Mutaciones administrativas realizadas por staff.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de audit">
        <label>
          Action
          <input
            value={action}
            onChange={(event) => setAction(event.target.value)}
            placeholder="installation_suspended"
            maxLength={100}
          />
        </label>
        <label>
          Result
          <select value={result} onChange={(event) => setResult(event.target.value)}>
            <option value="">All results</option>
            {results.map((item) => (
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
        <EmptyState title="No audit events" body="No hay eventos para los filtros actuales." />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Timestamp</th>
                <th>Actor</th>
                <th>Action</th>
                <th>Resource</th>
                <th>Organization</th>
                <th>Product</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((event: ControlAuditEvent) => (
                <tr key={event.id}>
                  <td>{formatDateTime(event.occurred_at)}</td>
                  <td>{event.actor?.display_name ?? "system"}</td>
                  <td>
                    <Link to={`/control/audit/${event.id}`} className="table-link">
                      {event.action}
                      <code>{event.id}</code>
                    </Link>
                  </td>
                  <td>
                    {event.resource_type}
                    {event.resource_label ? ` · ${event.resource_label}` : ""}
                  </td>
                  <td>{event.organization?.display_name ?? "—"}</td>
                  <td>{event.product?.name ?? "—"}</td>
                  <td>
                    <span className={`badge badge--${event.result}`}>{event.result}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function AuditDetailPage() {
  const { eventId = "" } = useParams();
  const query = useQuery({
    queryKey: ["control", "audit", eventId],
    queryFn: () => controlClient.auditEvent(eventId),
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
          <Link to="/control/audit" className="back-link">
            Audit
          </Link>
          <p className="eyebrow">{event.resource_type}</p>
          <h1>{event.action}</h1>
          <p>{event.id}</p>
        </div>
        <span className={`badge badge--${event.result}`}>{event.result}</span>
      </div>
      <dl className="details">
        <div>
          <dt>Timestamp</dt>
          <dd>{formatDateTime(event.occurred_at)}</dd>
        </div>
        <div>
          <dt>Actor</dt>
          <dd>{event.actor?.display_name ?? "system"}</dd>
        </div>
        <div>
          <dt>Resource</dt>
          <dd>
            {event.resource_type} · {event.resource_label ?? event.resource_id ?? "—"}
          </dd>
        </div>
        <div>
          <dt>Organization</dt>
          <dd>{event.organization?.display_name ?? "—"}</dd>
        </div>
        <div>
          <dt>Product</dt>
          <dd>{event.product?.name ?? "—"}</dd>
        </div>
        <div>
          <dt>Result</dt>
          <dd>{event.result}</dd>
        </div>
      </dl>
      {event.actor && (
        <dl className="details">
          <div>
            <dt>Email</dt>
            <dd>{event.actor.email}</dd>
          </div>
          <div>
            <dt>Status</dt>
            <dd>{event.actor.status}</dd>
          </div>
        </dl>
      )}
      <section className="related">
        <div>
          <h2>Metadata saneada</h2>
          <p>Sólo claves permitidas para esta acción.</p>
        </div>
        <pre>{JSON.stringify(event.metadata, null, 2)}</pre>
      </section>
    </section>
  );
}
