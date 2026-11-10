import { FormEvent, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { controlClient, type Product, type ProductStatus, type ProductType } from "../../api/control-client";
import { useCapability } from "../../auth/session";
import { ControlDialog } from "../../components/dialog";
import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";

import { formatDate, formatDateTime } from "../../lib/format-date";
import {
  PRODUCT_DESCRIPTION_MAX,
  PRODUCT_KEY_MAX,
  PRODUCT_NAME_MAX,
  isSensitiveStatusChange,
  productErrorCopy,
  validateProductDraft,
} from "./product-validation";

const statuses: ProductStatus[] = ["development", "active", "maintenance", "deprecated", "retired"];
const types: ProductType[] = ["desktop", "saas", "web", "mobile", "service", "integration", "internal"];
const statusLabel = (value: string) => value.replaceAll("_", " ");

export function ProductsPage() {
  const [params, setParams] = useSearchParams();
  const canManage = useCapability("products.manage");
  const status = (params.get("status") || undefined) as ProductStatus | undefined;
  const type = (params.get("type") || undefined) as ProductType | undefined;
  const cursor = params.get("cursor") || undefined;
  const query = useQuery({ queryKey: ["control", "products", { status, type, cursor }], queryFn: () => controlClient.products({ status, type, cursor }) });

  if (query.isPending) return <LoadingState label="Cargando products…" />;
  if (query.isError) return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  const update = (key: "status" | "type", value: string) => { const next = new URLSearchParams(); if (key === "status" ? value : status) next.set("status", key === "status" ? value : status!); if (key === "type" ? value : type) next.set("type", key === "type" ? value : type!); setParams(next); };
  const nextCursor = query.data.next_cursor ?? undefined;
  return <section className="page"><div className="page__heading"><div><p className="eyebrow">Registry</p><h1>Products</h1><p>Productos y proyectos operativos registrados en Platform.</p></div></div><div className="filters" aria-label="Filtros de products"><label>Status<select value={status ?? ""} onChange={(e) => update("status", e.target.value)}><option value="">All statuses</option>{statuses.map((item) => <option key={item} value={item}>{statusLabel(item)}</option>)}</select></label><label>Type<select value={type ?? ""} onChange={(e) => update("type", e.target.value)}><option value="">All types</option>{types.map((item) => <option key={item} value={item}>{statusLabel(item)}</option>)}</select></label></div>{canManage && <NewProductForm />}{query.data.items.length === 0 ? <EmptyState title="No products found" body="No hay registros que coincidan con los filtros actuales." /> : <div className="table-wrap"><table><thead><tr><th>Product</th><th>Type</th><th>Status</th><th>Updated</th></tr></thead><tbody>{query.data.items.map((product) => <tr key={product.id}><td><Link to={`/control/products/${product.key}`} className="table-link">{product.name}<code>{product.key}</code></Link></td><td>{product.product_type}</td><td><span className={`badge badge--${product.status}`}>{product.status}</span></td><td>{formatDate(product.updated_at)}</td></tr>)}</tbody></table></div>}{nextCursor && <button className="button button--secondary" onClick={() => setParams({ ...(status && { status }), ...(type && { type }), cursor: nextCursor })}>Next page</button>}</section>;
}

function NewProductForm() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [error, setError] = useState("");
  const create = useMutation({
    mutationFn: controlClient.createProduct,
    onSuccess: async (product) => {
      setError("");
      await queryClient.invalidateQueries({ queryKey: ["control", "products"] });
      navigate(`/control/products/${product.key}`);
    },
    onError: (failure) => setError(productErrorCopy(failure)),
  });

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const key = String(data.get("key") ?? "").trim();
    const name = String(data.get("name") ?? "");
    const description = String(data.get("description") ?? "");
    const invalid = validateProductDraft({ key, name, description });
    if (invalid) {
      setError(invalid);
      return;
    }
    setError("");
    create.mutate({
      key,
      name: name.trim(),
      description,
      status: String(data.get("status") ?? "development") as ProductStatus,
      type: String(data.get("type") ?? "saas") as ProductType,
    });
  };

  return (
    <form className="filters" aria-label="Crear producto" noValidate onSubmit={submit}>
      <label>
        Key
        <input name="key" required maxLength={PRODUCT_KEY_MAX} placeholder="nuevo-producto" autoComplete="off" />
      </label>
      <label>
        Name
        <input name="name" required maxLength={PRODUCT_NAME_MAX} placeholder="Nuevo producto" />
      </label>
      <label>
        Type
        <select name="type" defaultValue="saas">
          {types.map((item) => (
            <option key={item} value={item}>
              {statusLabel(item)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Status
        <select name="status" defaultValue="development">
          {statuses.map((item) => (
            <option key={item} value={item}>
              {statusLabel(item)}
            </option>
          ))}
        </select>
      </label>
      <label>
        Description
        <textarea name="description" maxLength={PRODUCT_DESCRIPTION_MAX} rows={2} />
      </label>
      <button className="button button--primary" type="submit" disabled={create.isPending}>
        {create.isPending ? "Creating…" : "Create product"}
      </button>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
    </form>
  );
}

export function ProductDetailPage() {
  const { productKey = "" } = useParams();
  const product = useQuery({ queryKey: ["control", "product", productKey], queryFn: () => controlClient.product(productKey) });
  const projects = useQuery({ queryKey: ["control", "projects", productKey], queryFn: () => controlClient.projects(productKey), enabled: product.isSuccess });
  const canManage = useCapability("products.manage");
  if (product.isPending) return <LoadingState label="Cargando product…" />;
  if (product.isError) return <RequestErrorState error={product.error} retry={() => void product.refetch()} />;
  return <section className="page"><div className="page__heading"><div><Link to="/control/products" className="back-link">Products</Link><p className="eyebrow">{product.data.product_type}</p><h1>{product.data.name}</h1><p>{product.data.description || "Sin descripción."}</p></div><span className={`badge badge--${product.data.status}`}>{product.data.status}</span></div><dl className="details"><div><dt>Key</dt><dd><code>{product.data.key}</code></dd></div><div><dt>Updated</dt><dd>{formatDateTime(product.data.updated_at)}</dd></div></dl>{canManage && <ProductEditForm key={product.data.updated_at} product={product.data} />}<section className="related"><div><h2>Projects</h2><p>Proyectos asociados a este product.</p></div>{projects.isPending ? <LoadingState label="Cargando projects…" /> : projects.isError ? <RequestErrorState error={projects.error} retry={() => void projects.refetch()} /> : projects.data.items.length === 0 ? <EmptyState title="No associated projects" body="Este product todavía no tiene projects asociados." /> : <div className="table-wrap"><table><thead><tr><th>Project</th><th>Type</th><th>Status</th></tr></thead><tbody>{projects.data.items.map((project) => <tr key={project.id}><td><strong>{project.name}</strong><code>{project.key}</code></td><td>{project.project_type}</td><td><span className={`badge badge--${project.status}`}>{project.status}</span></td></tr>)}</tbody></table></div>}</section></section>;
}

function ProductEditForm({ product }: { product: Product }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(product.name);
  const [description, setDescription] = useState(product.description);
  const [status, setStatus] = useState<ProductStatus>(product.status);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [saved, setSaved] = useState(false);

  const nextName = name.trim();
  const nameChanged = nextName !== product.name;
  const descriptionChanged = description !== product.description;
  const statusChanged = isSensitiveStatusChange(product.status, status);
  const dirty = nameChanged || descriptionChanged || statusChanged;

  const save = useMutation({
    mutationFn: () =>
      controlClient.updateProduct(product.key, {
        ...(nameChanged && { name: nextName }),
        ...(descriptionChanged && { description }),
        ...(statusChanged && { status }),
      }),
    onSuccess: async () => {
      setError("");
      setSaved(true);
      setConfirming(false);
      await queryClient.invalidateQueries({ queryKey: ["control", "product", product.key] });
      await queryClient.invalidateQueries({ queryKey: ["control", "products"] });
    },
    onError: (failure) => {
      setConfirming(false);
      setSaved(false);
      setError(productErrorCopy(failure));
    },
  });

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSaved(false);
    if (!dirty) return;
    const invalid = validateProductDraft({ name, description });
    if (invalid) {
      setError(invalid);
      return;
    }
    setError("");
    if (statusChanged) {
      setConfirming(true);
      return;
    }
    save.mutate();
  };

  return (
    <section className="panel" aria-label="Editar producto">
      <h2>Edit product</h2>
      <p>La clave y el tipo son inmutables. Cada cambio queda auditado con tu cuenta staff.</p>
      <form className="filters" noValidate onSubmit={submit}>
        <label>
          Name
          <input
            name="name"
            required
            maxLength={PRODUCT_NAME_MAX}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label>
          Status
          <select value={status} onChange={(event) => setStatus(event.target.value as ProductStatus)}>
            {statuses.map((item) => (
              <option key={item} value={item}>
                {statusLabel(item)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Description
          <textarea
            name="description"
            maxLength={PRODUCT_DESCRIPTION_MAX}
            rows={2}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
        <button className="button button--primary" type="submit" disabled={!dirty || save.isPending}>
          {save.isPending ? "Saving…" : "Save changes"}
        </button>
      </form>
      {saved && !dirty && (
        <p role="status" className="muted">
          Cambios guardados.
        </p>
      )}
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {confirming && (
        <ControlDialog title="Confirm status change" onClose={() => setConfirming(false)}>
          <p>
            Vas a cambiar el estado de <strong>{product.name}</strong> de{" "}
            <code>{product.status}</code> a <code>{status}</code>. La operación queda registrada en la
            auditoría staff.
          </p>
          <div className="dialog__actions">
            <button className="button" type="button" onClick={() => setConfirming(false)}>
              Cancel
            </button>
            <button
              className="button button--danger"
              type="button"
              disabled={save.isPending}
              onClick={() => save.mutate()}
            >
              {save.isPending ? "Saving…" : "Confirm change"}
            </button>
          </div>
        </ControlDialog>
      )}
    </section>
  );
}
