# ADR-0002: Estrategia de autenticación comercial para P02.2

**Estado:** ACCEPTED e implementado para P02.2-A. **Implementación:** `feat/platform-commercial-auth-p022a`; detalles en [diseño P02.2](p022-authentication-design.md).

## Contexto

P02.1 ya modela cuentas comerciales, organizaciones, membresías y auditoría, pero no tiene autenticación. El token administrativo DEV (`HIQNET_PLATFORM_ADMIN_TOKEN`) es una protección operativa y no representa una cuenta. Se requiere autenticación segura, revocable, multitenant y compatible con desarrollo local, sin añadir servicios externos no aprobados.

## Opciones evaluadas

### A. OIDC administrado por proveedor externo

**Ventajas:** delega contraseñas, MFA, recuperación y rotación; reduce complejidad de seguridad; facilita SSO futuro.

**Inconvenientes:** dependencia externa, coste, latencia, configuración de desarrollo, gestión de secretos, limitaciones offline y acoplamiento a claims. Para el estado actual del proyecto, añade un componente externo sin necesidad probada.

### B. Identidad propia con Argon2id y sesiones persistidas

**Ventajas:** control total de datos, coste cero, desarrollo local simple, integración directa con `commercial_account`, auditoría y membresías, sin dependencias externas.

**Inconvenientes:** la seguridad depende del diseño y la operación; hay que implementar verificación de correo, recuperación, bloqueo y rotación de sesiones con cuidado.

## Decisión propuesta

Seleccionar **identidad propia con Argon2id y sesiones persistidas**, con posibilidad de añadir OIDC más adelante como vínculo adicional, no como reemplazo obligatorio.

Justificación: el proyecto es un control plane técnico con despliegue local y necesidades multitenant. La identidad propia permite verificar correo, revocar sesiones, auditar eventos y mantener separación con POS sin depender de un proveedor. OIDC puede añadirse después como opción de inicio de sesión, pero no se introduce ahora sin aprobación.

## Implicaciones

- Contraseñas solo como hashes Argon2id. Los parámetros deben ser documentados y ajustados al hardware disponible; no se almacenan contraseñas ni tokens en logs.
- Sesiones en PostgreSQL con hash de token, expiración, revocación y rotación. Cookies `HttpOnly`, `Secure`, `SameSite=Lax` o `Strict` según el flujo.
- El actor se deriva únicamente de la sesión verificada; nunca de IDs enviados por cliente.
- Endpoints de autenticación versionados, con rate limiting, respuestas normalizadas y auditoría de intentos.
- No se considera autenticación productiva hasta tener recuperación, verificación y pruebas completas.

## Riesgos pendientes

- Protección contra phishing, MFA y gestión de dispositivos quedan fuera de P02.2.
- La recuperación de cuenta debe diseñarse con tokens de un solo uso y expiración corta.
- La protección contra enumeración y fuerza bruta depende de límites por IP/cuenta y mensajes neutrales.
- La revocación inmediata requiere validar sesión en cada solicitud o mantener una lista de revocación en caché.

## Alternativas descartadas

- OIDC como única estrategia inicial: mayor dependencia y coste sin necesidad actual.
- Sesiones stateless firmadas sin persistencia: revocación y auditoría deficientes.
- Token Bearer DEV como identidad: no representa una cuenta y permite acceso transversal.
"