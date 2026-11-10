import createClient from 'openapi-fetch';

import type { paths } from './schema';

/**
 * El Portal nunca lee ni guarda la cookie de sesión. `credentials: include`
 * permite que el navegador adjunte `hiqnet_session` a las llamadas de API.
 */
export const api = createClient<paths>({
  baseUrl: '',
  credentials: 'include',
  // Resolver fetch en cada llamada permite interceptarlo en pruebas y evita
  // capturar una referencia obsoleta del runtime.
  fetch: (request) => globalThis.fetch(request),
});
