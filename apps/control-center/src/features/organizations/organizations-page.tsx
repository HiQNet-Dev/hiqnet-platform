import { FormEvent } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  controlClient,
  type OrganizationStatus,
} from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { applyDirectoryFilter, withDirectoryCursor } from "../directory/filters";
import { formatDate, formatDateTime } from "../../lib/format-date";

const organizationStatuses: OrganizationStatus[] = [
  "active",
  "legacy_unclaimed",
  "archived",
];

export function OrganizationsPage() {
  const [params, setParams] = useSearchParams();
  const status = (params.get("status") || undefined) as OrganizationStatus | undefined;
  const search = params.get("search") || undefined;
  const cursor = params.get("cursor") || undefined;
  const query = useQuery({
    queryKey: ["control", "organizations", { status, search, cursor }],
    queryFn: () => controlClient.organizations({ status, search, cursor }),
  });

  if (query.isPending) return <LoadingState label="Cargando organizations…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }

  const submitSearch = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setParams(applyDirectoryFilter(params, { search: String(data.get("search") ?? "") }));
  };
  const nextCursor = query.data.next_cursor ?? undefined;

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Directory</p>
          <h1>Organizations</h1>
          <p>Organizaciones cliente registradas en Platform.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de organizations">
        <label>
          Status
          <select
            value={status ?? ""}
            onChange={(event) =>
              setParams(applyDirectoryFilter(params, { status: event.target.value }))
            }
          >
            <option value="">All statuses</option>
            {organizationStatuses.map((item) => (
              <option key={item} value={item}>
                {item.replaceAll("_", " ")}
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
              placeholder="Nombre de la organización"
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
          title="No organizations found"
          body="No hay organizaciones que coincidan con los filtros actuales."
        />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Organization</th>
                <th>Status</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((organization) => (
                <tr key={organization.id}>
                  <td>
                    <Link
                      to={`/control/organizations/${organization.id}`}
                      className="table-link"
                    >
                      {organization.display_name}
                      <code>{organization.id}</code>
                    </Link>
                  </td>
                  <td>
                    <span className={`badge badge--${organization.status}`}>
                      {organization.status.replaceAll("_", " ")}
                    </span>
                  </td>
                  <td>{formatDate(organization.created_at)}</td>
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

export function OrganizationDetailPage() {
  const { organizationId = "" } = useParams();
  const query = useQuery({
    queryKey: ["control", "organization", organizationId],
    queryFn: () => controlClient.organization(organizationId),
  });

  if (query.isPending) return <LoadingState label="Cargando organization…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }
  const organization = query.data;

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/organizations" className="back-link">
            Organizations
          </Link>
          <p className="eyebrow">Organization</p>
          <h1>{organization.display_name}</h1>
          <p>{organization.id}</p>
        </div>
        <span className={`badge badge--${organization.status}`}>
          {organization.status.replaceAll("_", " ")}
        </span>
      </div>
      <dl className="details">
        <div>
          <dt>UUID</dt>
          <dd>
            <code>{organization.id}</code>
          </dd>
        </div>
        <div>
          <dt>Created</dt>
          <dd>{formatDateTime(organization.created_at)}</dd>
        </div>
      </dl>
      <section className="related">
        <div>
          <h2>Memberships</h2>
          <p>Historial de cuentas asociadas, incluidas las revocadas.</p>
        </div>
        {organization.memberships.length === 0 ? (
          <EmptyState title="No memberships" body="Esta organización no registra miembros." />
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Account</th>
                  <th>Email</th>
                  <th>Account status</th>
                  <th>Role</th>
                  <th>Joined</th>
                  <th>Revoked</th>
                </tr>
              </thead>
              <tbody>
                {organization.memberships.map((membership) => (
                  <tr key={membership.account_id}>
                    <td>{membership.display_name}</td>
                    <td>{membership.email}</td>
                    <td>
                      <span className={`badge badge--${membership.account_status}`}>
                        {membership.account_status}
                      </span>
                    </td>
                    <td>{membership.role}</td>
                    <td>{formatDate(membership.joined_at)}</td>
                    <td>{formatDateTime(membership.revoked_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
      <section className="related">
        <div>
          <h2>Products</h2>
          <p>Asociaciones de producto registradas para esta organización.</p>
        </div>
        {organization.products.length === 0 ? (
          <EmptyState
            title="No product associations"
            body="Esta organización no tiene productos asociados."
          />
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Product</th>
                  <th>Key</th>
                  <th>Lifecycle</th>
                  <th>Association</th>
                  <th>Activated</th>
                </tr>
              </thead>
              <tbody>
                {organization.products.map((product) => (
                  <tr key={product.product_id}>
                    <td>{product.product_name}</td>
                    <td>
                      <code>{product.product_key}</code>
                    </td>
                    <td>
                      <span className={`badge badge--${product.product_status}`}>
                        {product.product_status}
                      </span>
                    </td>
                    <td>
                      <span className={`badge badge--${product.association_status}`}>
                        {product.association_status}
                      </span>
                    </td>
                    <td>{formatDate(product.activated_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </section>
  );
}
