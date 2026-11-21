# Runbook de despliegue interno de HiQNet Platform

**Estado:** propuesta operativa; no se ha ejecutado ni validado un despliegue de staging o producción en este incremento.
**Alcance:** API Axum, Control Center y Customer Portal como aplicaciones 100 % web.
**Relacionado:** [Seguridad Beta](../02-development/platform-beta-security.md) · [Desarrollo local](../02-development/local.md) · [Backup y restauración](backup-restore.md) · [Notas de Beta](../02-development/platform-beta-notes.md)

> Ver [contrato de trazabilidad, entornos y hoja de ruta](release-train.md). Este procedimiento requiere autorización del entorno, no un tag Git ni un GitHub Release. Un build verde, un despliegue interno y un piloto comercial son decisiones distintas.

## 1. Topología

```text
admin.<dominio>   →  Control Center (estático)   ─┐
portal.<dominio>  →  Customer Portal (estático)  ─┼→  /api/*  →  Platform API (Axum)  →  PostgreSQL 17
```

Reglas que la topología debe respetar:

- Cada frontend usa **el mismo origen** para su API: llama siempre a `/api/...` en rutas relativas. El proxy del host correspondiente reenvía `/api/*` a la API.
- Ambos hosts deben tener el **mismo sitio registrable** (`admin.ejemplo.com` y `portal.ejemplo.com`, o rutas del mismo host). `hiqnet_staff_session` usa `SameSite=Strict`; funciona porque las mutaciones salen del mismo sitio.
- No se habilita CORS. No hay `Access-Control-Allow-Origin` y no debe añadirse en el proxy.
- La API no se publica en un tercer origen. Si se publicara, los navegadores necesitarían CORS y la política de cookies cambiaría.

## 2. Artefactos

| Artefacto | Origen | Publicación |
|---|---|---|
| Imagen de la API | `docker build -t hiqnet-platform:<tag> .` (Dockerfile multi-stage, usuario `hiqnet`, healthcheck `/ready`) | Registro privado o carga directa al host |
| Control Center | `pnpm --filter @hiqnet/control-center build` → `apps/control-center/dist` | Host estático con fallback SPA |
| Customer Portal | `pnpm --filter @hiqnet/platform-portal build` → `apps/portal/dist` | Host estático con fallback SPA |

Construir la imagen y los dos estáticos **del mismo SHA validado**. Promover la imagen por digest y verificar los hashes de los estáticos; el tag Docker de build no es autoridad ni una etiqueta Git.

Fallback SPA: cualquier ruta que no sea un archivo servido debe devolver `index.html` del frontend correspondiente. Las rutas del Control Center viven bajo `/control/*`; las del Portal en la raíz.

## 3. Configuración de la API

Valores **obligatorios** de producción (perfil Beta interna o externa):

| Variable | Valor | Motivo |
|---|---|---|
| `DATABASE_URL` | `postgres://…` con usuario de mínimo privilegio | Conexión |
| `PORT` | `8080` | Puerto interno |
| `HIQNET_PLATFORM_ENV` | `production` | Habilita `Secure` obligatorio y desactiva permisos DEV |
| `HIQNET_PLATFORM_COOKIE_SECURE` | `true` (o ausente: es el valor por defecto) | Cookies sólo por HTTPS |
| `HIQNET_PLATFORM_ALLOWED_ORIGIN` | `https://admin.<dominio>,https://portal.<dominio>` | Origen exacto de cada mutación; sin comodines |
| `HIQNET_PLATFORM_ADMIN_TOKEN` | **vacío** | Deshabilita mutaciones con token administrativo |
| `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE` | `false` | No emitir licencias DEV |
| `HIQNET_PLATFORM_ENABLE_DOCS` | `false` | No publicar `/docs` |
| `HIQNET_PLATFORM_DEV_BOOTSTRAP` | ausente/false | Sin cuentas sintéticas |
| `HIQNET_PLATFORM_DEV_STAFF_BOOTSTRAP` | ausente/false | Sin staff sintético |
| `HIQNET_PLATFORM_SESSION_TTL_HOURS` | `8` interna; `4` externa sugerido | Vida de sesión |
| `HIQNET_PLATFORM_AUTH_CONCURRENCY` | `4` | Cota de Argon2id simultáneo |
| `HIQNET_PLATFORM_ENROLLMENT_*` | valores por defecto | Enrolamiento |
| `RUST_LOG` | `info` | Observabilidad sin datos sensibles |

Prohibiciones explícitas:

- No desplegar con `HIQNET_PLATFORM_ENV=development`.
- No desplegar con `HIQNET_PLATFORM_COOKIE_SECURE=false` (la API lo rechaza fuera de development).
- El perfil base no incluye `HIQNET_PLATFORM_SIGNING_KEY_B64` ni habilita emisión. P2 debe definir y aprobar por separado un perfil de firma exclusivo de staging para P4: clave y grants no productivos, custodio, alcance sintético y verificación del flag de emisión aplicable. No reutilizar claves ni habilitar emisión productiva.

La API aplica las migraciones de `migrations/` al abrir el pool. Un fallo de migración impide el arranque: no hay estado intermedio.

## 4. Proxy y cabeceras

Ejemplo con Caddy. Ajusta dominios y rutas; no copies la CSP sin validarla.

```caddyfile
admin.ejemplo.com {
  encode zstd gzip

  header {
    Strict-Transport-Security "max-age=63072000; includeSubDomains"
    Content-Security-Policy "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"
    X-Frame-Options "DENY"
    X-Content-Type-Options "nosniff"
    Referrer-Policy "no-referrer"
  }

  handle /api/* {
    reverse_proxy api:8080
  }

  handle {
    root * /srv/control-center
    try_files {path} /index.html
    file_server
  }
}

portal.ejemplo.com {
  encode zstd gzip

  header {
    Strict-Transport-Security "max-age=63072000; includeSubDomains"
    Content-Security-Policy "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"
    X-Frame-Options "DENY"
    X-Content-Type-Options "nosniff"
    Referrer-Policy "no-referrer"
  }

  handle /api/* {
    reverse_proxy api:8080
  }

  handle {
    root * /srv/portal
    try_files {path} /index.html
    file_server
  }
}
```

Notas:

- Los dos frontends no usan estilos ni scripts en línea, por lo que la CSP propuesta no necesita `'unsafe-inline'`. **Verifica en el navegador antes de exponer**: si una dependencia inyecta estilos en línea, el ajuste mínimo es `style-src 'self' 'unsafe-inline'` y debe quedar documentado.
- La CSP del perfil interno **no se ha validado contra un proxy real**. Forma parte del checklist de despliegue.
- No reescribas `Origin` en el proxy: la API compara el valor exacto y un `Origin` reescrito invalida la política CSRF.

## 5. Procedimiento de despliegue

1. Confirma el commit a desplegar y que CI está verde en ese commit.
2. Reutiliza los artefactos construidos una sola vez desde ese SHA. Si aún no existen, constrúyelos ahora, registra sus hashes y no los reconstruyas en reintentos ni promociones posteriores.
3. Tras autorizar la promoción, entrega los estáticos a sus hosts y promueve la imagen por digest por el canal privado autorizado.
4. Arranca la API con la configuración de §3.
5. Espera `GET /ready` → `{"status":"ok","database":"up"}`. `/health` sólo prueba que el proceso responde; no demuestra base de datos disponible.
6. Verifica en el navegador:
   - `https://admin.<dominio>/control/login` carga y autentica.
   - Una mutación staff (por ejemplo, crear una suscripción) no devuelve `403 origin_not_allowed`.
   - `https://portal.<dominio>/login` carga y autentica.
   - La cookie `hiqnet_staff_session` aparece `HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/api/v1/control`.
   - La cookie `hiqnet_session` aparece `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/api`.
7. Comprueba que `/docs` responde 404 con `HIQNET_PLATFORM_ENABLE_DOCS=false`.
8. En staging usa identidades de QA aprovisionadas sin bootstrap DEV; en producción interna usa cuentas autorizadas. Registra revisión, operador, hora, artefactos, migraciones aplicadas y evidencia del smoke.

## 6. Reversión

El responsable de la promoción decide la reversión si fallan el smoke, la integridad o las comprobaciones de acceso. Ensayar la compatibilidad del binario anterior con el set de migraciones candidato (incluida `0012` cuando aplique); sin esa evidencia no asumir que basta revertir artefactos.

1. Si el esquema es compatible, restaura el digest anterior de la API y ambos estáticos de la revisión anterior.
2. La base de datos **no se revierte** automáticamente. Si el esquema no es compatible, no arranques el binario anterior contra esa base; seguir la estrategia de recuperación aprobada.
3. Reinicia la API y verifica `/ready`.
4. Si el problema es de datos, restaura desde backup en una base nueva siguiendo [Backup y restauración](backup-restore.md); no sobrescribas la base productiva sin autorización.
5. Revoca sesiones staff y comerciales afectadas si el incidente fue de seguridad.

Nunca ejecutes `docker compose down -v` sobre un entorno con datos.

## 7. Checklist de despliegue

- [ ] SHA exacto con CI verde; digest, hashes, set de migraciones y autorización registrados.
- [ ] `HIQNET_PLATFORM_ENV=production`.
- [ ] `HIQNET_PLATFORM_COOKIE_SECURE` no es `false`.
- [ ] `HIQNET_PLATFORM_ALLOWED_ORIGIN` contiene exactamente los orígenes de los frontends desplegados, sin comodines ni barras finales.
- [ ] `HIQNET_PLATFORM_ADMIN_TOKEN` vacío.
- [ ] `HIQNET_PLATFORM_ALLOW_DEV_ISSUANCE=false`.
- [ ] `HIQNET_PLATFORM_ENABLE_DOCS=false`.
- [ ] Bootstrap DEV deshabilitado y cuentas sintéticas ausentes.
- [ ] TLS activo y HSTS/CSP/XFO/XCTO/Referrer-Policy presentes.
- [ ] `/ready` en verde y `/docs` en 404.
- [ ] Login staff y comercial verificados en navegador real.
- [ ] Mutación staff y mutación comercial sin `origin_not_allowed`.
- [ ] Backup verificado antes de la ventana de despliegue.
- [ ] Procedimiento de reversión conocido por quien despliega.
