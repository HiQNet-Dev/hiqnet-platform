# ADR-0001: Identidad comercial como modelo interno de Platform

**Estado:** aceptado para P02.1 (2026-11-07). **Alcance:** dominio, aplicación y PostgreSQL; sin transporte comercial.

## Contexto

P00/P01 almacenan instalaciones y licencias con `organization_id` UUID, sin cuentas ni membresías. El token administrativo DEV no representa una cuenta autenticada; vincularlo a organizaciones abriría acceso transversal. Se deben conservar UUID y datos previos sin conceder titularidad automática.

## Decisión

Se introducen cuenta comercial, organización, membresía versionada y evento de auditoría append-only. Roles tipados `owner/admin/member` versión 1; no tabla de roles ni tabla de usuarios POS. Instalaciones previas se asocian a organizaciones `legacy_unclaimed` sin dueño. Nuevas instalaciones requieren organización existente. FK compuesta verifica pertenencia de licencias a la misma organización. Cambios de membresías sólo por `owner`, con bloqueo por organización, protección de último dueño y auditoría en la misma transacción. Consultas siempre usan actor autenticado + membresía activa en SQL y ocultan existencia de tenants ajenos.

## Consecuencias y alternativas

- Sin autenticación P02.2 no se publica API comercial ni se permite reclamar organizaciones legadas; el servicio interno sólo acepta actor procedente de una futura capa confiable.
- Un backend podría invocar incorrectamente el servicio pasando IDs de cliente: P02.2 deberá imponer actor desde sesión y pruebas de extremo a extremo. No se declara seguridad de sesión terminada en P02.1.
- Al no introducir borrado en cascada ni política de retención irreversible, las referencias comerciales/auditorías impiden borrado físico; pendiente definir retención, recuperación de membresías revocadas y verificación de titularidad legada.
- Se descartó confiar en UUID de frontend, derivar cuentas de instalaciones, reutilizar token DEV como identidad, crear tabla de roles sin necesidad y asignar dueños arbitrarios durante migración.
