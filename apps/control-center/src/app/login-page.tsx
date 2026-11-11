import { FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { controlClient } from "../api/control-client";
import { useRefreshSession } from "../auth/session";
import { Brand } from "../components/brand";

export function LoginPage() {
  const navigate = useNavigate();
  const refresh = useRefreshSession();
  const [error, setError] = useState<string>();
  const login = useMutation({
    mutationFn: ({ email, password }: { email: string; password: string }) =>
      controlClient.login(email, password),
  });
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(undefined);
    const data = new FormData(event.currentTarget);
    try {
      await login.mutateAsync({
        email: String(data.get("email")),
        password: String(data.get("password")),
      });
      await refresh();
      navigate("/control/products", { replace: true });
    } catch {
      setError("No fue posible iniciar sesión. Verifica tus credenciales.");
    }
  }
  return (
    <main className="login">
      <aside className="login__brand">
        <Brand />
        <div>
          <h2>El plano de control de tu infraestructura HiQNet.</h2>
          <p>
            Organizaciones, productos, suscripciones e instalaciones en un solo lugar. Acceso
            restringido al personal autorizado de HiQNet.
          </p>
        </div>
        <footer>Internal · Platform</footer>
      </aside>
      <div className="login__form">
        <form className="login__panel" onSubmit={submit}>
          <img
            className="login__mark"
            src="/brand/hiqnet-mark.webp"
            alt=""
            width={56}
            height={56}
            aria-hidden="true"
          />
          <p className="eyebrow">HiQNet · internal</p>
          <h1>Control Center</h1>
          <p>Acceso exclusivo para personal autorizado.</p>
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
          <label>
            Email
            <input name="email" type="email" autoComplete="username" required />
          </label>
          <label>
            Contraseña
            <input name="password" type="password" autoComplete="current-password" required />
          </label>
          <button className="button button--primary" disabled={login.isPending}>
            {login.isPending ? "Verificando…" : "Iniciar sesión"}
          </button>
        </form>
      </div>
    </main>
  );
}
