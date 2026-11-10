import { FormEvent, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  controlClient,
  type CommercialSubscription,
  type CommercialSubscriptionStatus,
} from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { formatDateTime } from "../../lib/format-date";

const statuses: CommercialSubscriptionStatus[] = [
  "active",
  "suspended",
  "cancelled",
  "expired",
];

function inputDate(offsetDays = 0) {
  const date = new Date();
  date.setDate(date.getDate() + offsetDays);
  return date.toISOString().slice(0, 16);
}

function toRfc3339(value: string) {
  return new Date(value).toISOString();
}

export function SubscriptionsPage() {
  const queryClient = useQueryClient();
  const [status, setStatus] = useState<CommercialSubscriptionStatus | "">("");
  const [organizationId, setOrganizationId] = useState("");
  const [error, setError] = useState("");
  const subscriptions = useQuery({
    queryKey: ["control", "subscriptions", { status, organizationId }],
    queryFn: () =>
      controlClient.subscriptions({
        status: status || undefined,
        organizationId: organizationId || undefined,
      }),
  });
  const organizations = useQuery({
    queryKey: ["control", "organizations", "subscription-options"],
    queryFn: () => controlClient.organizations({}),
  });
  const products = useQuery({
    queryKey: ["control", "products", "subscription-options"],
    queryFn: () => controlClient.products({ limit: 100 }),
  });
  const plans = useQuery({
    queryKey: ["control", "plans", "subscription-options"],
    queryFn: () => controlClient.plans({ status: "active", limit: 100 }),
  });
  const create = useMutation({
    mutationFn: async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const data = new FormData(event.currentTarget);
      return controlClient.createSubscription(
        {
          organization_id: String(data.get("organization_id") ?? ""),
          product_id: String(data.get("product_id") ?? ""),
          plan_id: String(data.get("plan_id") ?? ""),
          starts_at: toRfc3339(String(data.get("starts_at") ?? "")),
          ends_at: toRfc3339(String(data.get("ends_at") ?? "")),
        },
        crypto.randomUUID(),
      );
    },
    onSuccess: async () => {
      setError("");
      await queryClient.invalidateQueries({ queryKey: ["control", "subscriptions"] });
    },
    onError: (failure) => setError(failure.message),
  });

  if (subscriptions.isPending) return <LoadingState label="Cargando subscriptions…" />;
  if (subscriptions.isError) {
    return (
      <RequestErrorState
        error={subscriptions.error}
        retry={() => void subscriptions.refetch()}
      />
    );
  }

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Commercial</p>
          <h1>Subscriptions</h1>
          <p>Contratos organización–producto–plan con historial y transiciones explícitas.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de subscriptions">
        <label>
          Status
          <select
            value={status}
            onChange={(event) =>
              setStatus(event.target.value as CommercialSubscriptionStatus | "")
            }
          >
            <option value="">All statuses</option>
            {statuses.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label>
          Organization
          <select
            value={organizationId}
            onChange={(event) => setOrganizationId(event.target.value)}
          >
            <option value="">All organizations</option>
            {(organizations.data?.items ?? []).map((organization) => (
              <option key={organization.id} value={organization.id}>
                {organization.display_name}
              </option>
            ))}
          </select>
        </label>
      </div>
      <form
        className="filters"
        aria-label="Crear subscription"
        onSubmit={(event) => create.mutate(event)}
      >
        <label>
          Organization
          <select name="organization_id" required defaultValue="">
            <option value="" disabled>
              Select organization
            </option>
            {(organizations.data?.items ?? []).map((organization) => (
              <option key={organization.id} value={organization.id}>
                {organization.display_name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Product
          <select name="product_id" required defaultValue="">
            <option value="" disabled>
              Select product
            </option>
            {(products.data?.items ?? []).map((product) => (
              <option key={product.id} value={product.id}>
                {product.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Plan
          <select name="plan_id" required defaultValue="">
            <option value="" disabled>
              Select plan
            </option>
            {(plans.data?.items ?? []).map((plan) => (
              <option key={plan.id} value={plan.id}>
                {plan.product.name} · {plan.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Starts
          <input name="starts_at" type="datetime-local" defaultValue={inputDate()} required />
        </label>
        <label>
          Ends
          <input name="ends_at" type="datetime-local" defaultValue={inputDate(30)} required />
        </label>
        <button className="button" type="submit" disabled={create.isPending}>
          {create.isPending ? "Creating…" : "Create subscription"}
        </button>
      </form>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {subscriptions.data.items.length === 0 ? (
        <EmptyState title="No subscriptions" body="No hay suscripciones para los filtros." />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Subscription</th>
                <th>Plan</th>
                <th>Status</th>
                <th>Starts</th>
                <th>Ends</th>
              </tr>
            </thead>
            <tbody>
              {subscriptions.data.items.map((subscription) => (
                <tr key={subscription.id}>
                  <td>
                    <Link
                      to={`/control/subscriptions/${subscription.id}`}
                      className="table-link"
                    >
                      {subscription.plan.name}
                      <code>{subscription.id}</code>
                    </Link>
                  </td>
                  <td>{subscription.plan.key}</td>
                  <td>
                    <span className={`badge badge--${subscription.status}`}>
                      {subscription.status}
                    </span>
                  </td>
                  <td>{formatDateTime(subscription.starts_at)}</td>
                  <td>{formatDateTime(subscription.ends_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function SubscriptionDetailPage() {
  const { subscriptionId = "" } = useParams();
  const queryClient = useQueryClient();
  const [error, setError] = useState("");
  const subscription = useQuery({
    queryKey: ["control", "subscription", subscriptionId],
    queryFn: () => controlClient.subscription(subscriptionId),
  });
  const plans = useQuery({
    queryKey: ["control", "plans", "renew-options"],
    queryFn: () => controlClient.plans({ status: "active", limit: 100 }),
  });
  const refresh = async (value: CommercialSubscription) => {
    setError("");
    await queryClient.invalidateQueries({
      queryKey: ["control", "subscription", subscriptionId],
    });
    await queryClient.invalidateQueries({ queryKey: ["control", "subscriptions"] });
    return value;
  };
  const transition = useMutation({
    mutationFn: (action: "suspend" | "reactivate" | "cancel") => {
      if (action === "suspend") return controlClient.suspendSubscription(subscriptionId);
      if (action === "cancel") return controlClient.cancelSubscription(subscriptionId);
      return controlClient.reactivateSubscription(subscriptionId);
    },
    onSuccess: refresh,
    onError: (failure) => setError(failure.message),
  });
  const renew = useMutation({
    mutationFn: async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const data = new FormData(event.currentTarget);
      return controlClient.renewSubscription(
        subscriptionId,
        {
          plan_id: String(data.get("plan_id") ?? "") || undefined,
          ends_at: toRfc3339(String(data.get("ends_at") ?? "")),
        },
        crypto.randomUUID(),
      );
    },
    onSuccess: refresh,
    onError: (failure) => setError(failure.message),
  });

  if (subscription.isPending) return <LoadingState label="Cargando subscription…" />;
  if (subscription.isError) {
    return (
      <RequestErrorState
        error={subscription.error}
        retry={() => void subscription.refetch()}
      />
    );
  }
  const value = subscription.data.subscription;
  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/subscriptions" className="back-link">
            Subscriptions
          </Link>
          <p className="eyebrow">{value.plan.key}</p>
          <h1>{value.plan.name}</h1>
          <p>{value.id}</p>
        </div>
        <span className={`badge badge--${value.status}`}>{value.status}</span>
      </div>
      <dl className="details">
        <div>
          <dt>Organization</dt>
          <dd>{value.organization_id}</dd>
        </div>
        <div>
          <dt>Product</dt>
          <dd>{value.product_id}</dd>
        </div>
        <div>
          <dt>Starts</dt>
          <dd>{formatDateTime(value.starts_at)}</dd>
        </div>
        <div>
          <dt>Ends</dt>
          <dd>{formatDateTime(value.ends_at)}</dd>
        </div>
        <div>
          <dt>Trial</dt>
          <dd>{value.is_trial ? "Yes" : "No"}</dd>
        </div>
        <div>
          <dt>Renewed from</dt>
          <dd>{value.renewed_from_subscription_id ?? "—"}</dd>
        </div>
      </dl>
      <div className="filters" aria-label="Transiciones">
        <button
          className="button"
          type="button"
          disabled={value.status !== "active" || transition.isPending}
          onClick={() => transition.mutate("suspend")}
        >
          Suspend
        </button>
        <button
          className="button"
          type="button"
          disabled={value.status !== "suspended" || transition.isPending}
          onClick={() => transition.mutate("reactivate")}
        >
          Reactivate
        </button>
        <button
          className="button button--danger"
          type="button"
          disabled={
            (value.status !== "active" && value.status !== "suspended") ||
            transition.isPending
          }
          onClick={() => transition.mutate("cancel")}
        >
          Cancel
        </button>
      </div>
      <form className="filters" aria-label="Renovar suscripción" onSubmit={(event) => renew.mutate(event)}>
        <label>
          New plan
          <select name="plan_id" defaultValue="">
            <option value="">Keep current plan</option>
            {(plans.data?.items ?? [])
              .filter((plan) => plan.product.id === value.product_id)
              .map((plan) => (
                <option key={plan.id} value={plan.id}>
                  {plan.name}
                </option>
              ))}
          </select>
        </label>
        <label>
          New end
          <input name="ends_at" type="datetime-local" defaultValue={inputDate(60)} required />
        </label>
        <button
          className="button"
          type="submit"
          disabled={
            renew.isPending ||
            (value.status !== "active" && value.status !== "expired")
          }
        >
          Renew
        </button>
      </form>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <section className="related">
        <div>
          <h2>History</h2>
          <p>Eventos append-only de la suscripción.</p>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Event</th>
                <th>From</th>
                <th>To</th>
                <th>Occurred</th>
                <th>Reason</th>
              </tr>
            </thead>
            <tbody>
              {subscription.data.events.map((event) => (
                <tr key={event.id}>
                  <td>{event.event}</td>
                  <td>{event.from_status ?? "—"}</td>
                  <td>{event.to_status ?? "—"}</td>
                  <td>{formatDateTime(event.occurred_at)}</td>
                  <td>{event.reason ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </section>
  );
}
