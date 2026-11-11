import { Link, useParams } from 'react-router-dom';

import { Badge } from '../../components/Badge';
import { EmptyState } from '../../components/EmptyState';
import { ErrorBanner } from '../../components/ErrorBanner';
import { PageHeader } from '../../components/PageHeader';
import { Skeleton } from '../../components/Skeleton';
import { useOrganizationProduct, useOrganizationProducts, type PortalProduct } from './queries';

function subscriptionTone(status: string | undefined) {
  if (status === 'active') return 'success' as const;
  if (status === 'suspended') return 'warning' as const;
  if (status === 'cancelled' || status === 'expired') return 'danger' as const;
  return 'neutral' as const;
}

function ProductRows({
  organizationId,
  items,
}: {
  organizationId: string;
  items: PortalProduct[];
}) {
  return (
    <div className="table-wrap">
      <table className="data-table">
        <thead>
          <tr>
            <th scope="col">Producto</th>
            <th scope="col">Plan</th>
            <th scope="col">Suscripción</th>
            <th scope="col">Derechos</th>
            <th scope="col">Vigencia</th>
            <th scope="col" className="data-table__action">
              <span className="sr-only">Abrir</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr key={item.product.id}>
              <td>
                <Link
                  className="table-primary"
                  to={`/app/organizations/${organizationId}/products/${item.product.id}`}
                >
                  {item.product.name}
                </Link>
                <code className="mono-id">{item.product.key}</code>
              </td>
              <td>{item.subscription?.plan.name ?? '—'}</td>
              <td>
                <Badge tone={subscriptionTone(item.subscription?.status)}>
                  {item.subscription?.status ?? 'sin suscripción'}
                </Badge>
              </td>
              <td>
                <Badge tone={item.entitlements.effective ? 'success' : 'warning'}>
                  {item.entitlements.effective ? 'efectivos' : item.entitlements.reason}
                </Badge>
              </td>
              <td>
                {item.subscription
                  ? `${new Date(item.subscription.starts_at).toLocaleDateString()} → ${new Date(
                      item.subscription.ends_at,
                    ).toLocaleDateString()}`
                  : '—'}
              </td>
              <td className="data-table__action">
                <Link
                  className="icon-button"
                  to={`/app/organizations/${organizationId}/products/${item.product.id}`}
                  aria-label={`Abrir ${item.product.name}`}
                >
                  →
                </Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function ProductsPage() {
  const { organizationId } = useParams();
  const query = useOrganizationProducts(organizationId);

  return (
    <div className="page-stack">
      <PageHeader
        level={2}
        eyebrow="Comercial"
        title="Productos contratados"
        description="Productos habilitados para esta organización, su plan, estado de suscripción y derechos efectivos."
      />
      {query.isPending ? (
        <div className="panel">
          <Skeleton className="skeleton--row" />
          <Skeleton className="skeleton--row" />
        </div>
      ) : query.isError ? (
        <ErrorBanner error={query.error} title="No pudimos cargar los productos" />
      ) : query.data.items.length === 0 ? (
        <EmptyState
          title="Sin productos contratados"
          description="Esta organización no tiene productos habilitados."
        />
      ) : (
        <div className="panel panel--flush">
          <ProductRows organizationId={organizationId ?? ''} items={query.data.items} />
        </div>
      )}
    </div>
  );
}

export function ProductDetailPage() {
  const { organizationId, productId } = useParams();
  const query = useOrganizationProduct(organizationId, productId);

  if (query.isPending) {
    return (
      <div className="page-stack">
        <Skeleton className="skeleton--title" />
        <Skeleton className="skeleton--block" />
      </div>
    );
  }
  if (query.isError) {
    return <ErrorBanner error={query.error} title="No pudimos abrir este producto" />;
  }
  const value = query.data;
  return (
    <div className="page-stack">
      <PageHeader
        level={2}
        eyebrow="Producto contratado"
        title={value.product.name}
        description={`${value.product.key} · ${value.organization_product_status}`}
        actions={
          <Link
            className="button button--secondary"
            to={`/app/organizations/${organizationId}/products`}
          >
            Volver
          </Link>
        }
      />
      <section className="panel">
        <div className="panel__heading">
          <div>
            <p className="eyebrow">Suscripción</p>
            <h2>{value.subscription?.plan.name ?? 'Sin suscripción'}</h2>
          </div>
          <Badge tone={subscriptionTone(value.subscription?.status)}>
            {value.subscription?.status ?? 'sin suscripción'}
          </Badge>
        </div>
        {value.subscription ? (
          <dl className="detail-list">
            <div>
              <dt>Plan</dt>
              <dd>
                {value.subscription.plan.name} · <code>{value.subscription.plan.key}</code>
              </dd>
            </div>
            <div>
              <dt>Inicio</dt>
              <dd>{new Date(value.subscription.starts_at).toLocaleString()}</dd>
            </div>
            <div>
              <dt>Fin</dt>
              <dd>{new Date(value.subscription.ends_at).toLocaleString()}</dd>
            </div>
            <div>
              <dt>Trial</dt>
              <dd>{value.subscription.is_trial ? 'Sí' : 'No'}</dd>
            </div>
          </dl>
        ) : (
          <p>No existe una suscripción para este producto.</p>
        )}
      </section>
      <section className="panel">
        <div className="panel__heading">
          <div>
            <p className="eyebrow">Derechos efectivos</p>
            <h2>{value.entitlements.effective ? 'Activos' : 'No efectivos'}</h2>
          </div>
          <Badge tone={value.entitlements.effective ? 'success' : 'warning'}>
            {value.entitlements.reason}
          </Badge>
        </div>
        <dl className="detail-list">
          <div>
            <dt>Features</dt>
            <dd>
              {Object.keys(value.entitlements.features).length === 0
                ? '—'
                : Object.entries(value.entitlements.features)
                    .map(([key, enabled]) => `${key}: ${enabled ? 'sí' : 'no'}`)
                    .join(' · ')}
            </dd>
          </div>
          <div>
            <dt>Límites</dt>
            <dd>
              {Object.keys(value.entitlements.limits).length === 0
                ? '—'
                : Object.entries(value.entitlements.limits)
                    .map(([key, limit]) => `${key}: ${limit}`)
                    .join(' · ')}
            </dd>
          </div>
        </dl>
      </section>
    </div>
  );
}
