import { createContext } from 'react';

import type { components } from '../api/schema';

export type AuthenticatedAccount = components['schemas']['MeResponse'];
export type AuthStatus = 'loading' | 'anonymous' | 'authenticated' | 'error';

export type AuthContextValue = {
  account: AuthenticatedAccount | null;
  status: AuthStatus;
  error: unknown;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  refetch: () => Promise<void>;
};

export const AuthContext = createContext<AuthContextValue | null>(null);
