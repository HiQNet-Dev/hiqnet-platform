# Backup y restauración de PostgreSQL

**Estado:** procedimiento ensayado en B9 (2026-11-09) sobre bases sintéticas.
**Alcance:** base de datos de HiQNet Platform (`PostgreSQL 17`).
**Relacionado:** [Despliegue](deployment.md) · [Desarrollo local](../02-development/local.md) · [Seguridad Beta](../02-development/platform-beta-security.md)

## 1. Qué respaldar

| Objeto | Incluido en `pg_dump` | Notas |
|---|---|---|
| Esquema y datos (`organization`, `product`, `plan`, `subscription`, `installation`, auditoría) | Sí | Es la fuente de verdad de Platform |
| `_sqlx_migrations` | Sí | Permite verificar coherencia del esquema restaurado |
| Secretos de firma de licencias | **No** | Viven en la configuración del proceso, no en la base |
| Configuración del proxy y variables de entorno | **No** | Respáldalos por separado en el gestor de secretos |

Un backup contiene **datos personales y comerciales**. No se sube a Git, no se comparte por canales informales y debe cifrarse cuando salga del host. `scripts/check_repository.py` rechaza dumps versionados.

## 2. Frecuencia recomendada para la Beta

La Beta no tiene un RPO formal. Mínimo operativo:

- Un dump diario automatizado con retención de 7 días.
- Un dump manual antes de cada ventana de despliegue.
- Un ensayo de restauración antes de exponer clientes externos.

Registra la fecha, el commit desplegado y el tamaño del dump. No midas el éxito sólo por el tamaño: verifica conteos (§5).

## 3. Crear el backup

Formato custom (`-Fc`), que permite restauración selectiva y compresión:

```bash
# En el host, con un usuario que pueda leer toda la base.
pg_dump --format=custom --no-owner --no-privileges \
        --file=/ruta/segura/hiqnet-platform-$(date +%Y%m%dT%H%M%S).dump \
        "$DATABASE_URL"
```

Con Docker Compose, evitando redirecciones binarias por shell:

```bash
docker compose exec -T postgres pg_dump -U hiqnet -d hiqnet -Fc -f /tmp/hiqnet.dump
docker cp hiqnet-platform-postgres:/tmp/hiqnet.dump .dev-data/hiqnet.dump
```

Verificación inmediata del dump:

```bash
pg_restore --list /ruta/segura/hiqnet-platform-<fecha>.dump | head -20
```

Si `pg_restore --list` falla, el dump no sirve: repite el backup antes de continuar.

## 4. Restaurar en una base nueva (procedimiento seguro)

Nunca restaures encima de la base productiva sin autorización explícita. El camino seguro es **restaurar en una base desechable**, verificar y sólo entonces promover.

```bash
# 1. Crear una base vacía con nombre distinto.
psql "$ADMIN_URL" -c 'CREATE DATABASE hiqnet_restore_check'

# 2. Restaurar con --exit-on-error para no dejar un esquema parcial silencioso.
pg_restore --exit-on-error --no-owner --no-privileges \
           --dbname="$RESTORE_URL" \
           /ruta/segura/hiqnet-platform-<fecha>.dump

# 3. Verificar (§5).
```

Promoción (sólo con autorización y ventana de mantenimiento):

1. Detén la API.
2. Renombra la base actual a `hiqnet_prev_<fecha>` en lugar de borrarla.
3. Restaura el dump en una base con el nombre productivo.
4. Arranca la API y comprueba `/ready`.
5. Si algo falla, vuelve a la base anterior renombrándola de nuevo.

No uses `--clean` contra una base con datos.

## 5. Verificación de integridad

```sql
-- Migraciones aplicadas: debe coincidir con el número de archivos de migrations/.
SELECT count(*) AS migrations, max(version) AS last_version FROM _sqlx_migrations;

-- Volumen de negocio y auditoría.
SELECT
  (SELECT count(*) FROM organization)        AS organizations,
  (SELECT count(*) FROM product)             AS products,
  (SELECT count(*) FROM plan)                AS plans,
  (SELECT count(*) FROM subscription)        AS subscriptions,
  (SELECT count(*) FROM installation)        AS installations,
  (SELECT count(*) FROM staff_audit_event)    AS staff_audit,
  (SELECT count(*) FROM commercial_security_event) AS security_events;

-- Integridad referencial mínima.
SELECT count(*) FROM subscription s
 LEFT JOIN organization o ON o.id = s.organization_id
 WHERE o.id IS NULL;  -- debe ser 0
```

Compara los conteos con la base de origen. Una diferencia en tablas append-only (auditoría) o en `subscription` indica un backup incompleto.

## 6. Restauración puntual de una tabla

`pg_restore` permite extraer un objeto concreto de un dump custom:

```bash
pg_restore --data-only --table=subscription --dbname="$RESTORE_URL" backup.dump
```

Úsalo sólo en una base desechable para comparar; no lo apliques sobre producción sin revisar el efecto en `subscription_event` y en las restricciones de unicidad.

## 7. Evidencia del ensayo en B9

Ejecutado el 2026-11-09 sobre bases sintéticas:

1. `pg_dump` de la base de pruebas en formato custom.
2. `pg_restore --exit-on-error` en una base nueva.
3. Comparación de conteos: `migrations=11`, `plan=19`, `subscription=12`, `staff_audit_event=613` coincidentes con el origen.
4. La API arrancó contra la base restaurada y respondió `/ready` con `database=up`.

Además, CI ejecuta en cada push un ciclo equivalente (`pg_dump` → `pg_restore` → verificación de una fila sembrada) dentro del job `Rust (fmt, clippy, tests)`.

## 8. Limitaciones conocidas de la Beta

- No hay backups automáticos ni retención gestionada por Platform: es responsabilidad del host.
- No hay restauración a un punto en el tiempo (PITR). Depende de la configuración de PostgreSQL del host.
- `TRUNCATE` no está bloqueado por los triggers append-only; un backup reciente es la mitigación aceptada.
- Los dumps no incluyen claves de firma ni secretos de configuración.
