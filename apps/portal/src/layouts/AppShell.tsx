import { useState } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Building2, KeyRound, LogOut, Menu, ShieldCheck, X } from 'lucide-react';

import { useAuth } from '../auth/useAuth';
import { useToast } from '../components/useToast';
import { BrandMark } from '../components/BrandMark';
import { cn } from '../utils/cn';

const navigation = [
  { to: '/app/organizations', label: 'Organizaciones', icon: Building2 },
  { to: '/enroll', label: 'Enrolar dispositivo', icon: KeyRound },
  { to: '/account/security', label: 'Cuenta y seguridad', icon: ShieldCheck },
];

export function AppShell() {
  const auth = useAuth();
  const toast = useToast();
  const navigate = useNavigate();
  const location = useLocation();
  const [menuOpen, setMenuOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);

  const closeMenu = () => setMenuOpen(false);

  async function handleLogout() {
    setLoggingOut(true);
    try {
      await auth.logout();
      toast.push('Sesión cerrada.', 'success');
      navigate('/login', { replace: true });
    } catch {
      toast.push('No fue posible cerrar la sesión.', 'error');
    } finally {
      setLoggingOut(false);
    }
  }

  return (
    <div className="app-shell">
      <button
        type="button"
        className={cn('sidebar-backdrop', menuOpen && 'sidebar-backdrop--visible')}
        aria-label="Cerrar navegación"
        onClick={closeMenu}
      />
      <aside className={cn('sidebar', menuOpen && 'sidebar--open')}>
        <div className="sidebar__brand">
          <BrandMark compact />
          <div>
            <strong>HiQNet</strong>
            <span>Platform Portal</span>
          </div>
          <button
            type="button"
            className="icon-button sidebar__close"
            onClick={closeMenu}
            aria-label="Cerrar navegación"
          >
            <X size={18} />
          </button>
        </div>

        <nav className="sidebar__nav" aria-label="Navegación principal">
          {navigation.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              onClick={closeMenu}
              className={({ isActive }) => cn('sidebar__link', isActive && 'sidebar__link--active')}
            >
              <Icon size={18} aria-hidden="true" />
              <span>{label}</span>
            </NavLink>
          ))}
        </nav>

        <div className="sidebar__footer">
          <div className="sidebar__account">
            <span className="avatar" aria-hidden="true">
              {auth.account?.email.slice(0, 1).toUpperCase() ?? '?'}
            </span>
            <div>
              <strong>{auth.account?.email ?? 'Cuenta'}</strong>
              <span>Cuenta comercial</span>
            </div>
          </div>
          <button
            type="button"
            className="sidebar__logout"
            onClick={() => void handleLogout()}
            disabled={loggingOut}
          >
            <LogOut size={17} aria-hidden="true" />
            {loggingOut ? 'Cerrando…' : 'Cerrar sesión'}
          </button>
        </div>
      </aside>

      <div className="app-shell__main">
        <header className="topbar">
          <button
            type="button"
            className="icon-button topbar__menu"
            onClick={() => setMenuOpen(true)}
            aria-label="Abrir navegación"
          >
            <Menu size={20} />
          </button>
          <div className="topbar__context">
            <span>Portal comercial</span>
            <strong>
              {location.pathname.startsWith('/account')
                ? 'Cuenta y seguridad'
                : location.pathname.startsWith('/enroll')
                  ? 'Enrolar dispositivo'
                  : 'Organizaciones'}
            </strong>
          </div>
          <span className="topbar__status">
            <span className="status-dot" aria-hidden="true" />
            Sesión segura
          </span>
        </header>
        <main className="app-content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
