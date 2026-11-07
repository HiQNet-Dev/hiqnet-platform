# Reglas de HiQNet Platform

- Rust core/application contiene decisiones de dominio; adaptador PostgreSQL contiene SQL y handlers Axum sólo HTTP/composición.
- Conserva migraciones SQLx ya publicadas; añade migraciones incrementales. Nunca borres volúmenes de desarrollo por defecto.
- No publiques `.env`, dumps, tokens, claves, datos privados ni documentos internos. Usa fixtures sintéticos en bases aisladas.
- `/health` indica proceso HTTP; `/ready` exige roundtrip real a PostgreSQL. No confundir ambos estados.
- Mantén commits, comentarios y documentación técnica en español; identificadores Rust en inglés.
- PR y CI antes de fusionar. No publiques tags ni releases sin validación y autorización; cambios administrativos requieren consentimiento explícito.
- Ejecuta fmt, Clippy, pruebas con PostgreSQL aislado y revisa diff/archivos prohibidos antes del PR.
