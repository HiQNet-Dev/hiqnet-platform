import { EmptyState } from "../../components/states";

export function OrganizationsPage() {
  return <section className="page"><div className="page__heading"><div><p className="eyebrow">Control Center</p><h1>Organizations</h1><p>Vista global de organizaciones.</p></div></div><EmptyState title="Módulo disponible en C4" body="La API administrativa global de Organizations aún no existe. Esta foundation no consulta ni reutiliza endpoints comerciales." /></section>;
}
