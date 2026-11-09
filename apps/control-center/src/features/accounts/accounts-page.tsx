import { FormEvent } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { controlClient, type AccountStatus } from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { applyDirectoryFilter, withDirectoryCursor } from "../directory/filters";
import { formatDate, formatDateTime } from "../../lib/format-date";

const accountStatuses: AccountStatus[] = ["active", "disabled"];

export function AccountsPage() {
  const [params, setParams] = useSearchParams();
  const status = (params.get("status") || undefined) as AccountStatus | undefined;
  const search = params.get("search") || undefined;
  const cursor = params.get("cursor") || undefined;
  const query = useQuery({
    queryKey: ["control", "accounts", { status, search, cursor }],
    queryFn: () => controlClient.accounts({ status, search, cursor }),
  });

  if (query.isPending) return <LoadingState label="Cargando accounts…" />;
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
          <h1>Accounts</h1>
          <p>Cuentas comerciales globales registradas en Platform.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de accounts">
        <label>
          Status
          <select
            value={status ?? ""}
            onChange={(event) =>
              setParams(applyDirectoryFilter(params, { status: event.target.value }))
            }
          >
            <option value="">All statuses</option>
            {accountStatuses.map((item) => (
              <option key={item} value={item}>
                {item}
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
              placeholder="Email o nombre"
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
          title="No accounts found"
          body="No hay cuentas que coincidan con los filtros actuales."
        />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Account</th>
                <th>Email</th>
                <th>Status</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {query.data.items.map((account) => (
                <tr key={account.id}>
                  <td>
                    <Link to={`/control/accounts/${account.id}`} className="table-link">
                      {account.display_name}
                    </Link>
                  </td>
                  <td>{account.email}</td>
                  <td>
                    <span className={`badge badge--${account.status}`}>{account.status}</span>
                  </td>
                  <td>{formatDate(account.created_at)}</td>
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

export function AccountDetailPage() {
  const { accountId = "" } = useParams();
  const query = useQuery({
    queryKey: ["control", "account", accountId],
    queryFn: () => controlClient.account(accountId),
  });

  if (query.isPending) return <LoadingState label="Cargando account…" />;
  if (query.isError) {
    return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  }
  const account = query.data;

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/accounts" className="back-link">
            Accounts
          </Link>
          <p className="eyebrow">Account</p>
          <h1>{account.display_name}</h1>
          <p>{account.email}</p>
        </div>
        <span className={`badge badge--${account.status}`}>{account.status}</span>
      </div>
      <dl className="details">
        <div>
          <dt>UUID</dt>
          <dd>
            <code>{account.id}</code>
          </dd>
        </div>
        <div>
          <dt>Created</dt>
          <dd>{formatDateTime(account.created_at)}</dd>
        </div>
      </dl>
      <section className="related">
        <div>
          <h2>Memberships</h2>
          <p>Organizaciones asociadas, incluidas las memberships revocadas.</p>
        </div>
        {account.memberships.length === 0 ? (
          <EmptyState
            title="No memberships"
            body="Esta cuenta no registra membresías."
          />
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Organization</th>
                  <th>Organization status</th>
                  <th>Role</th>
                  <th>Joined</th>
                  <th>Revoked</th>
                </tr>
              </thead>
              <tbody>
                {account.memberships.map((membership) => (
                  <tr key={membership.organization_id}>
                    <td>
                      <Link
                        to={`/control/organizations/${membership.organization_id}`}
                        className="table-link"
                      >
                        {membership.organization_display_name}
                      </Link>
                    </td>
                    <td>
                      <span className={`badge badge--${membership.organization_status}`}>
                        {membership.organization_status.replaceAll("_", " ")}
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
    </section>
  );
}
