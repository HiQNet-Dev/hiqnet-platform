# Beta local: validación conservadora

Este procedimiento no publica ni despliega nada. Usa sólo fixtures sintéticos; no uses bases del usuario, producción ni el volumen de desarrollo para ensayos. No hace falta iniciar infraestructura para el gate local.

## Sin PostgreSQL

Desde la raíz, con Rust, Python, Node 24, pnpm 11 y dependencias del workspace ya disponibles:

```bash
devkit doctor
devkit gates list
devkit gates check local --yes
python3 scripts/check_repository.py
python3 scripts/check_size_policy.py
```

El gate `local` ejecuta fmt y Clippy de Rust, pruebas Python y lint, formato (Portal), typecheck, pruebas unitarias y comprobación de contrato de ambos frontends. No ejecuta tests Rust que requieren base de datos, E2E, builds ni auditorías de dependencias; un gate verde **no** equivale a Beta validada. Revisa además `git diff --check`, `git status --short` y el diff antes de abrir PR. No incluyas dumps, credenciales ni archivos `.env` reales.

## PostgreSQL aislado (sólo con autorización y bases preparadas)

`cargo test --workspace` ejecuta pruebas que migran bases y dejan algunos eventos sintéticos persistentes; **no lo ejecutes** sin confirmar previamente que `TEST_DATABASE_URL`, `TEST_UPGRADE_DATABASE_URL` y `TEST_AUTH_UPGRADE_DATABASE_URL` apuntan a tres bases distintas, desechables y dedicadas a tests (nombres que incluyan `test`). No reutilices `DATABASE_URL` ni una base con datos valiosos. No imprimas las URLs ni cargues secretos en logs. La [guía local](../02-development/local.md#pruebas-aisladas) describe la preparación y CI crea sus propias bases efímeras. Si no hay PostgreSQL aislado disponible, marca estas pruebas **BLOCKED**, no PASS; no arranques un servicio ni alteres bases para forzarlas.

La API aplica migraciones SQLx al abrir la conexión. Para investigar deriva, compara **sin escribir** `migrations/` con las versiones, checksums y estado de `_sqlx_migrations` de una copia o base sintética; no edites migraciones publicadas (`0001`–`0012`) para “arreglar” checksums. Si hay discrepancia, detén el ensayo, conserva evidencia sin datos sensibles y revisa la causa antes de cualquier intervención. No borres volúmenes, no uses `down -v`, `DROP DATABASE`, `TRUNCATE` ni `pg_restore --clean` contra una base existente. Un ensayo de restauración se hace en una base nueva con nombre distinto siguiendo [backup y restore](backup-restore.md); promoverla o modificar una base real requiere autorización explícita.
