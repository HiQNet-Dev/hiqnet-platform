export function LoadingState({ label = "Cargando datos…" }: { label?: string }) {
  return <div className="state state--loading" role="status">{label}</div>;
}
export function ErrorState({ title = "No fue posible cargar esta vista", retry }: { title?: string; retry?: () => void }) {
  return <section className="state state--error" role="alert"><h2>{title}</h2><p>Comprueba tu conexión e inténtalo de nuevo.</p>{retry && <button className="button" onClick={retry}>Reintentar</button>}</section>;
}
export function EmptyState({ title, body }: { title: string; body: string }) {
  return <section className="state"><h2>{title}</h2><p>{body}</p></section>;
}
export function RequestErrorState({ error, retry }: { error: unknown; retry: () => void }) {
  const status = typeof error === "object" && error !== null && "status" in error ? (error as { status?: number }).status : undefined;
  return status === 403
    ? <ErrorState title="Acceso insuficiente" />
    : <ErrorState retry={retry} />;
}
