import { FormEvent, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  controlClient,
  type CommercialPlan,
  type CommercialPlanStatus,
} from "../../api/control-client";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";
import { formatDateTime } from "../../lib/format-date";

const statuses: CommercialPlanStatus[] = [
  "draft",
  "active",
  "maintenance",
  "deprecated",
  "retired",
];

function parseObject(value: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(value || "{}");
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Debe ser un objeto JSON.");
  }
  return parsed as Record<string, unknown>;
}

export function PlansPage() {
  const queryClient = useQueryClient();
  const [productId, setProductId] = useState("");
  const [status, setStatus] = useState<CommercialPlanStatus | "">("");
  const [error, setError] = useState("");
  const plans = useQuery({
    queryKey: ["control", "plans", { productId, status }],
    queryFn: () =>
      controlClient.plans({
        productId: productId || undefined,
        status: status || undefined,
      }),
  });
  const products = useQuery({
    queryKey: ["control", "products", "plan-options"],
    queryFn: () => controlClient.products({ limit: 100 }),
  });
  const create = useMutation({
    mutationFn: async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const data = new FormData(event.currentTarget);
      return controlClient.createPlan({
        product_id: String(data.get("product_id") ?? ""),
        key: String(data.get("key") ?? "").trim(),
        name: String(data.get("name") ?? "").trim(),
        description: String(data.get("description") ?? ""),
        status: String(data.get("status") ?? "draft"),
        is_trial: data.get("is_trial") === "on",
        features: parseObject(String(data.get("features") ?? "{}")),
        limits: parseObject(String(data.get("limits") ?? "{}")),
      });
    },
    onSuccess: async () => {
      setError("");
      await queryClient.invalidateQueries({ queryKey: ["control", "plans"] });
    },
    onError: (failure) => setError(failure.message),
  });

  if (plans.isPending) return <LoadingState label="Cargando plans…" />;
  if (plans.isError) {
    return <RequestErrorState error={plans.error} retry={() => void plans.refetch()} />;
  }

  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <p className="eyebrow">Commercial</p>
          <h1>Plans</h1>
          <p>Catálogo comercial por producto: features y límites declarativos.</p>
        </div>
      </div>
      <div className="filters" aria-label="Filtros de plans">
        <label>
          Product
          <select value={productId} onChange={(event) => setProductId(event.target.value)}>
            <option value="">All products</option>
            {(products.data?.items ?? []).map((product) => (
              <option key={product.id} value={product.id}>
                {product.name} ({product.key})
              </option>
            ))}
          </select>
        </label>
        <label>
          Status
          <select
            value={status}
            onChange={(event) => setStatus(event.target.value as CommercialPlanStatus | "")}
          >
            <option value="">All statuses</option>
            {statuses.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
      </div>
      <form className="filters" aria-label="Crear plan" onSubmit={(event) => create.mutate(event)}>
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
          Key
          <input name="key" required maxLength={64} placeholder="pro" />
        </label>
        <label>
          Name
          <input name="name" required maxLength={200} placeholder="Pro" />
        </label>
        <label>
          Status
          <select name="status" defaultValue="draft">
            {statuses.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label>
          Features JSON
          <input name="features" defaultValue='{"sync":true}' />
        </label>
        <label>
          Limits JSON
          <input name="limits" defaultValue='{"seats":10}' />
        </label>
        <label>
          Trial
          <input name="is_trial" type="checkbox" />
        </label>
        <button className="button" type="submit" disabled={create.isPending}>
          {create.isPending ? "Creating…" : "Create plan"}
        </button>
      </form>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {plans.data.items.length === 0 ? (
        <EmptyState title="No plans" body="No hay planes para los filtros actuales." />
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Plan</th>
                <th>Product</th>
                <th>Status</th>
                <th>Trial</th>
                <th>Subscriptions</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {plans.data.items.map((plan) => (
                <tr key={plan.id}>
                  <td>
                    <Link to={`/control/plans/${plan.id}`} className="table-link">
                      {plan.name}
                      <code>{plan.key}</code>
                    </Link>
                  </td>
                  <td>{plan.product.name}</td>
                  <td>
                    <span className={`badge badge--${plan.status}`}>{plan.status}</span>
                  </td>
                  <td>{plan.is_trial ? "Yes" : "No"}</td>
                  <td>{plan.subscription_count}</td>
                  <td>{formatDateTime(plan.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {plans.data.next_cursor && <p>Hay más planes; usa la API con cursor.</p>}
    </section>
  );
}

export function PlanDetailPage() {
  const { planId = "" } = useParams();
  const queryClient = useQueryClient();
  const [error, setError] = useState("");
  const plan = useQuery({
    queryKey: ["control", "plan", planId],
    queryFn: () => controlClient.plan(planId),
  });
  const update = useMutation({
    mutationFn: async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const data = new FormData(event.currentTarget);
      const body: Record<string, unknown> = {
        name: String(data.get("name") ?? "").trim(),
        description: String(data.get("description") ?? ""),
        status: String(data.get("status") ?? ""),
      };
      if (!plan.data || plan.data.subscription_count === 0) {
        body.features = parseObject(String(data.get("features") ?? "{}"));
        body.limits = parseObject(String(data.get("limits") ?? "{}"));
      }
      return controlClient.updatePlan(planId, body);
    },
    onSuccess: async (updated: CommercialPlan) => {
      setError("");
      queryClient.setQueryData(["control", "plan", planId], updated);
      await queryClient.invalidateQueries({ queryKey: ["control", "plans"] });
    },
    onError: (failure) => setError(failure.message),
  });
  if (plan.isPending) return <LoadingState label="Cargando plan…" />;
  if (plan.isError) {
    return <RequestErrorState error={plan.error} retry={() => void plan.refetch()} />;
  }
  const value = plan.data;
  return (
    <section className="page">
      <div className="page__heading">
        <div>
          <Link to="/control/plans" className="back-link">
            Plans
          </Link>
          <p className="eyebrow">{value.product.name}</p>
          <h1>{value.name}</h1>
          <p>
            <code>{value.key}</code> · {value.subscription_count} subscriptions
          </p>
        </div>
        <span className={`badge badge--${value.status}`}>{value.status}</span>
      </div>
      <form className="filters" onSubmit={(event) => update.mutate(event)}>
        <label>
          Name
          <input name="name" defaultValue={value.name} required maxLength={200} />
        </label>
        <label>
          Description
          <input name="description" defaultValue={value.description} />
        </label>
        <label>
          Status
          <select name="status" defaultValue={value.status}>
            {statuses.map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
        </label>
        <label>
          Features JSON
          <input
            name="features"
            defaultValue={JSON.stringify(value.features)}
            disabled={value.subscription_count > 0}
          />
        </label>
        <label>
          Limits JSON
          <input
            name="limits"
            defaultValue={JSON.stringify(value.limits)}
            disabled={value.subscription_count > 0}
          />
        </label>
        <button className="button" type="submit" disabled={update.isPending}>
          Save
        </button>
      </form>
      {value.subscription_count > 0 && (
        <p>Las condiciones de uso están congeladas porque el plan ya tiene subscriptions.</p>
      )}
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
    </section>
  );
}
