import { FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { controlClient } from "../api/control-client";
import { useRefreshSession } from "../auth/session";

export function LoginPage() {
  const navigate = useNavigate();
  const refresh = useRefreshSession();
  const [error, setError] = useState<string>();
  const login = useMutation({ mutationFn: ({ email, password }: { email: string; password: string }) => controlClient.login(email, password) });
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setError(undefined);
    const data = new FormData(event.currentTarget);
    try { await login.mutateAsync({ email: String(data.get("email")), password: String(data.get("password")) }); await refresh(); navigate("/control/products", { replace: true }); }
    catch { setError("No fue posible iniciar sesión. Verifica tus credenciales."); }
  }
  return <main className="login"><form className="login__panel" onSubmit={submit}><p className="eyebrow">HiQNet · internal</p><h1>Control Center</h1><p>Acceso exclusivo para personal autorizado.</p>{error && <p className="inline-error" role="alert">{error}</p>}<label>Email<input name="email" type="email" autoComplete="username" required /></label><label>Contraseña<input name="password" type="password" autoComplete="current-password" required /></label><button className="button button--primary" disabled={login.isPending}>{login.isPending ? "Verificando…" : "Iniciar sesión"}</button></form></main>;
}
