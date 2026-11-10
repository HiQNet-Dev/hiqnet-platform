# Control Center — Threat model (C0)

**Estado:** ACCEPTED (ratificado 2026-11-09)
**Fecha:** 2026-11-09
**Alcance:** Control Center, API `control` y modelo staff. No cubre POS local ni infraestructura cloud (fases futuras).

## 1. Activos

- Identidad y sesiones staff.
- Permisos internos (capabilities).
- Datos comerciales multi-tenant (organizaciones, cuentas, instalaciones).
- Auditoría staff (integridad y trazabilidad).
- Metadatos de producto/proyecto/entorno/despliegue.
- Secretos de configuración (nunca en claro en el panel).

## 2. Fronteras de confianza

```
Staff browser ──(cookie staffSession)──> Control API ──> PostgreSQL
                                          │
                                          └─ audit append-only
```

- El navegador no es autoridad.
- La API valida identidad, capability, recurso y entorno en cada operación.
- La vista global staff no elimina el aislamiento por tenant en las consultas.

## 3. Amenazas y mitigaciones

| #   | Amenaza                          | Ejemplo                                     | Mitigación propuesta                                                         |
| --- | -------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------------- |
| T1  | Cuenta staff comprometida        | Credencial filtrada                         | Argon2id, bloqueo, sesión corta, revocación inmediata, auditoría, MFA futuro |
| T2  | Escalada de privilegios          | Operator intenta `staff.manage`             | Capabilities server-side, deny-by-default, 403 neutral                       |
| T3  | Error de alcance cross-tenant    | Admin staff ve datos de otro tenant por bug | `organization_id` en SQL, constraints, tests de aislamiento                  |
| T4  | Confused deputy                  | Endpoint staff reutiliza sesión comercial   | Extractores separados; staff no acepta `commercialSession`                   |
| T5  | Acciones masivas destructivas    | Borrar N organizaciones                     | Confirmación tipada, límites, 4-eyes futuro, auditoría                       |
| T6  | Robo de sesión                   | XSS/CSRF                                    | Cookies HttpOnly+SameSite=Strict, Origin exacto, CSP, sin tokens en JS       |
| T7  | IDOR                             | Cambiar `:id` en URL                        | Autorización por recurso + 404 neutral                                       |
| T8  | Manipulación de auditoría        | Ocultar una acción                          | Tabla append-only (trigger), sin UPDATE/DELETE, revisión de integridad       |
| T9  | Exposición de secretos           | Mostrar API keys                            | Sólo `configured`, `last rotated`, `key id`; nunca valor                     |
| T10 | Suplantación de soporte          | Ver como cliente                            | No implementado; requiere diseño, consentimiento y auditoría                 |
| T11 | Confusión de límites de producto | Módulo POS contamina el core                | Registry por producto; reglas específicas en módulos                         |
| T12 | Fuerza bruta / enumeración       | Login staff                                 | Rate limiting por correo/IP confiable, mensajes neutrales                    |
| T13 | Fuga en logs                     | Tokens/códigos en tracing                   | Sanitización; no loggear cuerpos sensibles ni URIs con secretos              |
| T14 | Dependencia runtime del POS      | Panel caído rompe ventas                    | POS local-first; Control Center no participa en la venta                     |

## 4. Controles base (C1)

- Security scheme OpenAPI `staffSession` distinto de `commercialSession` y `devAdminToken`.
- Sesión validada contra PostgreSQL en cada request; revocación efectiva inmediata.
- Deshabilitar `staff_account` revoca sesiones en la misma transacción.
- Auditoría obligatoria en toda mutación administrativa.
- Origin exacto en producción; `SameSite=Strict`; `Secure` salvo development.
- Rate limiting y bloqueo de credenciales.
- Respuestas `403`/`404` neutrales, sin filtrar existencia ni tenant.

## 5. Pruebas de seguridad requeridas

- Sesión comercial → **sin acceso** a `/api/v1/control/*`.
- `devAdminToken` → **sin acceso** a `/api/v1/control/*`.
- Staff sin capability → `403`.
- Staff deshabilitado → pierde acceso de inmediato.
- Aislamiento cross-tenant en consultas globales.
- Aislamiento cross-producto.
- Aislamiento cross-entorno cuando exista.

## 6. Riesgos aceptados en C1 (con seguimiento)

- Sin MFA: se mitiga con sesión corta, bloqueo y auditoría; MFA requiere ADR propio.
- Sin 4-eyes: se mitiga con confirmación tipada y límites; 4-eyes en fase posterior.
- Sin SSO empresarial: OIDC futuro como identidad externa adicional.

## 7. Decisiones pendientes

- [ ] Política de step-up authentication para acciones críticas.
- [ ] Revisión de CSP del Control Center.
- [ ] Definición de acciones que exigen 4-eyes.
