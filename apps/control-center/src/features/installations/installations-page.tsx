import { FormEvent } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  controlClient,
  type InstallationAdministrativeStatus,
  type InstallationIdentityStatus,
} from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { formatDate, formatDateTime } from "../../lib/format-date";
import { applyDirectoryFilter, withDirectoryCursor } from "../directory/filters";
import {
  administrativeStatusLabel,
  identityStatusLabel,
} from "./installation-format";

const administrativeStatuses: InstallationAdministrativeStatus[] = [
  "active",
  "suspended",
  "unregistered",
];
const identityStatuses: InstallationIdentityStatus[] = ["active", "unenrolled", "retired"];

export function InstallationsPage() {
  const [params, setParams] = useSearchParams();
  const status = (params.get("status") || undefined) as InstallationAdministrativeStatus | undefined;
  const identityStatus = (params.get("identity_status") || undefined) as
    | InstallationIdentityStatus
    | undefined;
  const search = params.get("search") || undefined;
  const cursor = params.get("cursor") || undefined;
  const query = useQuery({
    queryKey: [
      "control",
      "installations",
      { status, identityStatus, organizationId: undefined, productId: undefined, search, cursor },
    ],
    queryFn: () => controlClient.installations({ status, identityStatus, search, cursor }),
  });

  if (query.isPending) return <LoadingState label="Cargando installations…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }

  const submitSearch = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setParams(applyDirectoryFilter(params, { search: String(data.get("search") ?? "") }));
  };
  const nextCursor = query.data.next_cursor ?? undefined;
  const filtered = Boolean(status || identityStatus || search);

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">POS Control</p>
          <h1>Installations</h1>
          <p>Instalaciones POS registradas en Platform, con su organización y producto atribuidos.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de installations">
        <label>
          Administrative status
          <select
            value={status ?? ""}
            onChange={(event) =>
              setParams(applyDirectoryFilter(params, { status: event.target.value }))
            }
          >
            <option value="">All statuses</option>
            {administrativeStatuses.map((item) => (
              <option key={item} value={item}>
                {administrativeStatusLabel(item)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Identity status
          <select
            value={identityStatus ?? ""}
            onChange={(event) =>
              setParams(applyDirectoryFilter(params, { identityStatus: event.target.value }))
            }
          >
            <option value="">All identities</option>
            {identityStatuses.map((item) => (
              <option key={item} value={item}>
                {identityStatusLabel(item)}
              </option>
            ))}
          </select>
        </label>
        <form className="search-form" onSubmit={submitSearch} role="search">
          <label>
            Search
            <input
              key={search ?? ""}
              name="search"
              defaultValue={search ?? ""}
              placeholder="Device label"
              maxLength={200}
            />
          </label>
          <button className="button" type="submit">
            Search
          </button>
        </form>
      </div>
      {query.data.items.length === 0 ? (
        <EmptyState
          title={filtered ? "No installations match these filters" : "No installations yet"}
          body={
            filtered
              ? "No hay instalaciones que coincidan con los filtros actuales. Ajusta o limpia los filtros."
              : "Platform todavía no registra instalaciones POS."
          }
        />
      ) : (
        <div className="table-wrap">
          <table className="installations-table">
            <thead>
              <tr>
                <th>Device</th>
                <th>Organization</th>
                <th>Product</th>
                <th>Administrative</th>
                <th>Identity</th>
                <th>Registered</th>
                <th>Last known activity</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((installation) => (
                <tr key={installation.id}>
                  <td data-label="Device">
                    <Link
                      to={`/control/installations/${installation.id}`}
                      className="table-link"
                      title={installation.id}
                    >
                      {installation.device_label}
                      <code>{installation.id.slice(0, 8)}</code>
                    </Link>
                  </td>
                  <td data-label="Organization">
                    <Link
                      to={`/control/organizations/${installation.organization.id}`}
                      className="table-link"
                    >
                      {installation.organization.display_name}
                    </Link>
                  </td>
                  <td data-label="Product">
                    {installation.product ? (
                      <span className="stacked">
                        <span>{installation.product.name}</span>
                        <code>{installation.product.key}</code>
                      </span>
                    ) : (
                      <span className="muted">Sin producto asignado</span>
                    )}
                  </td>
                  <td data-label="Administrative">
                    <span className={`badge badge--${installation.administrative_status}`}>
                      {administrativeStatusLabel(installation.administrative_status)}
                    </span>
                  </td>
                  <td data-label="Identity">
                    <span className={`badge badge--${installation.identity_status}`}>
                      {identityStatusLabel(installation.identity_status)}
                    </span>
                  </td>
                  <td data-label="Registered">{formatDate(installation.registered_at)}</td>
                  <td data-label="Last known activity">
                    {formatDateTime(installation.last_known_activity_at)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {nextCursor && (
        <button
          className="button button--secondary"
          onClick={() => setParams(withDirectoryCursor(params, nextCursor))}
        >
          Next page
        </button>
      )}
    </section>
  );
}
