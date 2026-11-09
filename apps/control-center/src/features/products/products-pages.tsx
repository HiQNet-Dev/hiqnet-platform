import { Link, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { controlClient, type ProductStatus, type ProductType } from "../../api/control-client";

import { EmptyState, LoadingState, RequestErrorState } from "../../components/states";

const statuses: ProductStatus[] = ["development", "active", "maintenance", "deprecated", "retired"];
const types: ProductType[] = ["desktop", "saas", "web", "mobile", "service", "integration", "internal"];
const statusLabel = (value: string) => value.replaceAll("_", " ");

export function ProductsPage() {
  const [params, setParams] = useSearchParams();
  const status = (params.get("status") || undefined) as ProductStatus | undefined;
  const type = (params.get("type") || undefined) as ProductType | undefined;
  const cursor = params.get("cursor") || undefined;
  const query = useQuery({ queryKey: ["control", "products", { status, type, cursor }], queryFn: () => controlClient.products({ status, type, cursor }) });

  if (query.isPending) return <LoadingState label="Cargando products…" />;
  if (query.isError) return <RequestErrorState error={query.error} retry={() => void query.refetch()} />;
  const update = (key: "status" | "type", value: string) => { const next = new URLSearchParams(); if (key === "status" ? value : status) next.set("status", key === "status" ? value : status!); if (key === "type" ? value : type) next.set("type", key === "type" ? value : type!); setParams(next); };
  const nextCursor = query.data.next_cursor ?? undefined;
  return <section className="page"><div className="page__heading"><div><p className="eyebrow">Registry</p><h1>Products</h1><p>Productos y proyectos operativos registrados en Platform.</p></div></div><div className="filters" aria-label="Filtros de products"><label>Status<select value={status ?? ""} onChange={(e) => update("status", e.target.value)}><option value="">All statuses</option>{statuses.map((item) => <option key={item} value={item}>{statusLabel(item)}</option>)}</select></label><label>Type<select value={type ?? ""} onChange={(e) => update("type", e.target.value)}><option value="">All types</option>{types.map((item) => <option key={item} value={item}>{statusLabel(item)}</option>)}</select></label></div>{query.data.items.length === 0 ? <EmptyState title="No products found" body="No hay registros que coincidan con los filtros actuales." /> : <div className="table-wrap"><table><thead><tr><th>Product</th><th>Type</th><th>Status</th><th>Updated</th></tr></thead><tbody>{query.data.items.map((product) => <tr key={product.id}><td><Link to={`/control/products/${product.key}`} className="table-link">{product.name}<code>{product.key}</code></Link></td><td>{product.product_type}</td><td><span className={`badge badge--${product.status}`}>{product.status}</span></td><td>{new Intl.DateTimeFormat("es", { dateStyle: "medium" }).format(new Date(product.updated_at))}</td></tr>)}</tbody></table></div>}{nextCursor && <button className="button button--secondary" onClick={() => setParams({ ...(status && { status }), ...(type && { type }), cursor: nextCursor })}>Next page</button>}</section>;
}

export function ProductDetailPage() {
  const { productKey = "" } = useParams();
  const product = useQuery({ queryKey: ["control", "product", productKey], queryFn: () => controlClient.product(productKey) });
  const projects = useQuery({ queryKey: ["control", "projects", productKey], queryFn: () => controlClient.projects(productKey), enabled: product.isSuccess });
  if (product.isPending) return <LoadingState label="Cargando product…" />;
  if (product.isError) return <RequestErrorState error={product.error} retry={() => void product.refetch()} />;
  return <section className="page"><div className="page__heading"><div><Link to="/control/products" className="back-link">Products</Link><p className="eyebrow">{product.data.product_type}</p><h1>{product.data.name}</h1><p>{product.data.description || "Sin descripción."}</p></div><span className={`badge badge--${product.data.status}`}>{product.data.status}</span></div><dl className="details"><div><dt>Key</dt><dd><code>{product.data.key}</code></dd></div><div><dt>Updated</dt><dd>{new Date(product.data.updated_at).toLocaleString("es")}</dd></div></dl><section className="related"><div><h2>Projects</h2><p>Proyectos asociados a este product.</p></div>{projects.isPending ? <LoadingState label="Cargando projects…" /> : projects.isError ? <RequestErrorState error={projects.error} retry={() => void projects.refetch()} /> : projects.data.items.length === 0 ? <EmptyState title="No associated projects" body="Este product todavía no tiene projects asociados." /> : <div className="table-wrap"><table><thead><tr><th>Project</th><th>Type</th><th>Status</th></tr></thead><tbody>{projects.data.items.map((project) => <tr key={project.id}><td><strong>{project.name}</strong><code>{project.key}</code></td><td>{project.project_type}</td><td><span className={`badge badge--${project.status}`}>{project.status}</span></td></tr>)}</tbody></table></div>}</section></section>;
}
