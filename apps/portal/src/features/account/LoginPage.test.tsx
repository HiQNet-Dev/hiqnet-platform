import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';

import { AuthContext } from '../../auth/auth-context';
import type { AuthContextValue } from '../../auth/auth-context';
import { LoginPage } from './LoginPage';

describe('LoginPage', () => {
  it('envía credenciales y redirige al destino autenticado', async () => {
    const login = vi.fn(async () => undefined);
    const authValue: AuthContextValue = {
      account: null,
      status: 'anonymous',
      error: null,
      login,
      logout: vi.fn(async () => undefined),
      refetch: vi.fn(async () => undefined),
    };

    render(
      <MemoryRouter initialEntries={['/login']}>
        <AuthContext.Provider value={authValue}>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            <Route path="/app/organizations" element={<h1>Organizaciones</h1>} />
          </Routes>
        </AuthContext.Provider>
      </MemoryRouter>,
    );

    const user = userEvent.setup();
    await screen.findByRole('heading', { name: 'Inicia sesión' });
    await user.type(screen.getByLabelText('Correo electrónico'), 'owner@example.test');
    await user.type(screen.getByLabelText('Contraseña'), 'contraseña-larga-1');
    await user.click(screen.getByRole('button', { name: 'Entrar' }));

    expect(await screen.findByRole('heading', { name: 'Organizaciones' })).toBeInTheDocument();
    expect(login).toHaveBeenCalledWith('owner@example.test', 'contraseña-larga-1');
  });

  it('muestra el error de autenticación sin revelar detalles internos', async () => {
    const login = vi.fn(async () => {
      throw new Error('detalle interno');
    });
    const authValue: AuthContextValue = {
      account: null,
      status: 'anonymous',
      error: null,
      login,
      logout: vi.fn(async () => undefined),
      refetch: vi.fn(async () => undefined),
    };

    render(
      <MemoryRouter initialEntries={['/login']}>
        <AuthContext.Provider value={authValue}>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
          </Routes>
        </AuthContext.Provider>
      </MemoryRouter>,
    );

    const user = userEvent.setup();
    await user.type(screen.getByLabelText('Correo electrónico'), 'owner@example.test');
    await user.type(screen.getByLabelText('Contraseña'), 'incorrecta');
    await user.click(screen.getByRole('button', { name: 'Entrar' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'No fue posible completar la operación',
    );
    expect(screen.queryByText('detalle interno')).not.toBeInTheDocument();
  });
});
