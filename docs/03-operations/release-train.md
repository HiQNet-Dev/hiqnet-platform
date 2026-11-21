# Release train de HiQNet después de R2.1

**Decisión:** Platform se opera como aplicación web interna de HiQNet; POS se distribuye como producto instalable. Este documento establece la política y los criterios de avance; no certifica un despliegue, un piloto ni pruebas nativas.

## Platform: operación, no distribución

HiQNet opera API, Control Center y Customer Portal. Que exista un portal comercial no convierte a Platform en software que el usuario instala ni autoriza acceso externo. `platform-v0.1.0-beta.1` y un GitHub Release no son hitos operativos ni requisitos de disponibilidad. `0.1.0-beta.1` puede seguir como nombre documental del corte beta; el `0.1.0` del workspace Cargo sigue siendo metadato técnico. No se requiere bump. El estado se expresa por entorno y revisión, no por SemVer.

| Hito               | Evidencia que lo distingue                                                                                                                         |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Build              | Generó la imagen de API y los estáticos de ambas SPA desde un SHA; no implica que pasaron las pruebas ni que se ejecuten en un entorno.            |
| Validación CI      | El SHA exacto superó los checks requeridos y el gate de validación del artefacto; no cambia ningún entorno.                                        |
| Deployment         | Artefactos inmutables promovidos a un entorno autorizado, con revisión registrada y smoke satisfactorio.                                           |
| Piloto comercial   | Decisión separada: cohorte, alcance, fechas, soporte y autorización de datos/acceso. No equivale a emisión productiva ni a disponibilidad general. |
| Producción interna | Servicio oficial operado por HiQNet, con control de acceso, observabilidad y recuperación aprobados; no implica distribución ni piloto.            |

## Contrato de trazabilidad propuesto

Ejemplo conceptual (no corresponde a un despliegue real):

```yaml
deployment_id: staging-20261119-0001
environment: staging
deployment_revision: staging/revision-0001
git_sha: 0123456789abcdef0123456789abcdef01234567
migration_head: "0012"
migration_set: "manifest@sha256:<digest>"
image_digest: "registry.example/hiqnet-platform@sha256:<digest>"
static_artifacts:
  control_center: "sha256:<digest>"
  portal: "sha256:<digest>"
ci_run:
  url: "https://github.com/<org>/<repo>/actions/runs/<id>"
  attempt: 1
  conclusion: success
deployed_at: "2026-11-19T18:00:00Z"
deployed_by: "actor:<id-estable>"
```

- `deployment_id` es único por intento de promoción, incluso si se repite el mismo SHA; `deployment_revision` identifica la revisión efectiva del entorno. Si el proveedor no da una, usar un identificador inmutable del registro de operación.
- Registrar el SHA completo, el run y su intento comprobados contra ese SHA, el digest OCI del artefacto de API realmente desplegado (resuelto para el destino) y los hashes de ambos estáticos. Un tag Docker mutable o el nombre de la rama no bastan. En promoción no reconstruir los artefactos.
- El manifiesto contiene las versiones y checksums de migraciones esperadas. Confirmar el set aplicado y el último identificador exitoso contra `_sqlx_migrations`; `0012` es el head del árbol revisado, no evidencia de una BD desplegada. Bloquear promoción exitosa si no coinciden.
- `deployed_at` en UTC y `deployed_by` identifica al operador o cuenta de servicio; adjuntar autorización, smoke y revisión anterior. Registrar fallos/rollback por separado; no marcar activa una revisión fallida. Guardar el registro y evidencias en el historial operativo privado existente (o evidencia adjunta a CI si hay retención suficiente), sin secretos. No crear base de datos ni servicio para este contrato en P1.

## Gobernanza de `main`

Ruleset `26764207`: PR obligatorio, no eliminación ni actualizaciones non-fast-forward, `required_approving_review_count=0` mientras haya un solo desarrollador, sin bypass, y `strict_required_status_checks_policy=true`. En P1 se añadieron tres contextos a los dos existentes y se activó `strict`; el GET posterior coincidió con la propuesta. Contextos exactos del workflow `CI`, verificados en `origin/main` y en el PR #42:

- `Rust (fmt, clippy, tests)`
- `Archivos prohibidos`
- `Control Center (lint, typecheck, test, build, api, e2e)`
- `Portal (lint, tipos, tests, build)`
- `Portal E2E full-stack`

No renombrar esos jobs ni agregar filtros que omitan su ejecución en PRs sin coordinar con la política de GitHub. Un PR desactualizado debe validar de nuevo; cero aprobaciones no exime de revisión cuando haya otro revisor. El ruleset de tags `26764214` queda **DEPRECATED** como mecanismo operativo, **activo y sin cambios**: conserva protección contra update/deletion de `platform-v*`, pero no restringe creación. Sólo reconsiderar su retirada tras desactivar publicación en las refs mantenidas, revisar tags/refs históricas y contar con autorización explícita.

## R2.1: qué se conserva

| Componente                                                                   | Decisión              | P2: tratamiento                                                                                                                                                                                                                                                 |
| ---------------------------------------------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Backend, PostgreSQL 17, migraciones, upgrade, backup/restore y OpenAPI       | **REUSE**             | Mantener checks de CI: herramientas Rust, bases efímeras, snapshot OpenAPI y contrato.                                                                                                                                                                          |
| Portal y Control Center                                                      | **REUSE**             | Lint, tipos, tests, builds y drift OpenAPI; no perder E2E de Control Center ni sus viewports.                                                                                                                                                                   |
| E2E full-stack                                                               | **REUSE**             | CI con Axum y PostgreSQL reales y datos sintéticos. No sustituye el smoke de contenedor/TLS en staging.                                                                                                                                                         |
| Archivos prohibidos y política de tamaño                                     | **REUSE**             | Preservar `check_repository.py` y `check_size_policy.py`; tamaño no equivale a scan de seguridad.                                                                                                                                                               |
| Escaneo adicional de secretos y dependencias                                 | **ADAPT**             | Llevar a CI los controles útiles del `release-validation.yml` que sólo existe en `5f06cf6` (descartado); el grep actual tiene alcance limitado. Acordar gate de dependencias o excepciones; resultados históricos no son evidencia del SHA candidato.                                                          |
| `release-validation.yml`                                                     | **ADAPT**             | **No existe en `main`**: sólo en `5f06cf6`, descartado. P2 puede reimplementar su cobertura dentro de `ci.yml` o como workflow de validación de despliegue, sin duplicar jobs. Preservar dependencias de runner y Playwright.                                                                                                                       |
| Docker/identidad de artefactos                                               | **ADAPT**             | El Dockerfile de API se conserva. El build actual en release no hace push ni smoke; exigir build sin publicación en CI o gate interno del SHA y evidencia del smoke antes de promover. Identificar por separado API y estáticos.                                |
| `release-platform.yml`, trigger `platform-v*`, `publish` y `contents: write` | **DEPRECATE**         | Retirar la ruta de GitHub Release en un incremento autorizado; el SHA validado sí se conserva. `workflow_dispatch` ya es sin publicación, no es despliegue.                                                                                                     |
| Validación SemVer y tests de gating de release                               | **ADAPT / DEPRECATE** | En `main` CI descubre `test_validate_release.py`, `test_size_policy.py` y `test_release_postgres.py`. `test_release_gating.py` sólo existe en `5f06cf6` (descartado) y no debe buscarse en esta rama. conservar aislamiento y trazabilidad y probar ausencia de publicación. El contrato SemVer se conserva sólo si hace falta internamente. |

`5f06cf6` es evidencia histórica y punto de partida, **no se integra**: mantiene la publicación por tag. Vive sólo en la rama local `fix/platform-release-gating`; si esa rama desaparece, sus referencias dejan de ser resolubles. De ese trabajo se integró únicamente el delta documental de `5eef20a`. P2 reimplementa el alcance de validación desde `main`, sin reescribir historial. Mientras siga la ruta legacy, no crear `platform-v*`: una ref antigua puede contener el YAML antiguo y el sistema de tags no permite borrarla sin intervención administrativa.

## Entornos y secuencia Platform

| Entorno      | Uso y frontera                                                                                                                                                          |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `local`      | Desarrollo con servicios y fixtures aislados, sin datos ni credenciales reales. No es evidencia de promoción.                                                           |
| `staging`    | Validación integrada real, TLS, proxy y PostgreSQL aislado, configuración equivalente a producción sin secretos ni datos productivos; acceso limitado y fixtures de QA. |
| `production` | Aplicación interna oficial. Identidad, secretos, red y BD independientes; acceso autorizado, respaldo, monitoreo, alertas y operación con responsable.                  |

| Paso                                        | Criterio de avance                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **P1 governance**                           | Acordar política, revisar identidad y endurecer ruleset.                                                                                                                                                                                                                                                                 |
| **P2 staging deployment readiness**         | Validación sin publicación, artefactos por digest, requisitos de TLS/secretos/acceso/observabilidad, responsables y destino definidos, contrato de revisión, perfil de firma exclusivo de staging para P4, backup y restore ensayados, upgrade y compatibilidad de rollback comprobados para las migraciones aplicables. |
| **P3 staging deployment**                   | Autorización separada; promover a staging el conjunto exacto de artefactos y verificar `/ready`, cookies, TLS, fallback SPA, ambos logins/mutaciones y aislamiento entre tenants.                                                                                                                                        |
| **P4 POS ↔ Platform integrated validation** | Linux, Windows y Android contra staging; comprobar enrolamiento, grant firmado, reinicio offline, grant expirado/inválido, servidor no disponible, TLS y recuperación. Nada contra producción.                                                                                                                           |
| **P5 production-internal deployment**       | Con evidencia de staging/P4 y autorización separada, promover artefactos sin reconstruir y verificar/registrar la revisión; ningún piloto comercial implícito.                                                                                                                                                           |

El siguiente incremento POS es **R3 Native Validation Lab Setup**, previa autorización de instalaciones. [Plan POS](pos-native-validation-plan.md): R3 laboratorio nativo → R4 auditoría UX/documentación → R5 preparación `0.2.0-beta.1` → R6 release candidate → R7 tag/release. Para el procedimiento futuro de Platform, ver el [runbook](deployment.md).
