# G4.2 — Gestión de claves Ed25519: identidad de dispositivo y emisión de licencias

- **Propósito:** separar y custodiar las claves de identidad y de emisión sin comprometer la verificación offline.
- **Estado:** PROPOSED — no se generan claves reales en G4.2-A.
- **Fecha:** 2026-11-08.
- **Alcance:** separación, formato, custodia, rotación, revocación y compatibilidad de claves.
- **Decisiones:** §5, §6 y §9.
- **Dependencias:** ADR-0003, protocolo y modelo de datos.
- **Riesgos:** §9.
- **Referencias:** §10.

## 1. Dominios de clave separados

| Dominio | Genera | Custodia | Uso | Nunca |
|---|---|---|---|---|
| Identidad de dispositivo | POS | Equipo del cliente (keystore o archivo) | Firmar desafíos de enrolamiento | Firmar licencias; salir del equipo |
| Emisión de licencias | Platform | Infraestructura de Platform | Firmar `LicenseGrant` | Firmar desafíos; residir en el POS |
| Verificación de licencias | Platform (genera) / POS (embebe) | Clave pública | Verificar grants offline | Firmar |

La separación es obligatoria: comprometer una clave no debe comprometer la otra.

## 2. Formato y versionado

### 2.1 Clave pública de dispositivo

```text
ed25519:v1:<base64url(32 bytes)>
```

- `v1` identifica el algoritmo y la codificación, no la versión de la instalación.
- Longitud exacta: 32 bytes.
- `key_id` propuesto: `base64url(sha256(public_key)[0..16])`, es decir, 16 bytes (128 bits) de huella.
- La huella completa SHA-256 se guarda en PostgreSQL (`fingerprint`), pero se expone el `key_id` truncado.
- Cambiar el formato exige nueva versión y rechazo explícito de versiones desconocidas.

### 2.2 Clave de emisión

- Ed25519; clave pública embebida en el POS.
- El grant actual (`LicenseGrant` v1) no incluye `key_id`. Para rotación se propone:
  - Emitir con `key_id` en un campo de extensión o en la cabecera de transporte, sin alterar la forma canónica del grant v1.
  - O versionar el grant a v2 con `key_id`, manteniendo verificación de v1 durante una ventana.
- Decisión PENDIENTE (D-F).

## 3. Custodia en el POS

| Plataforma | Almacén preferido | Fallback |
|---|---|---|
| Windows | DPAPI (por usuario/máquina según despliegue) | Archivo con ACL restrictiva |
| Linux | libsecret / Secret Service si existe | Archivo `0600` |
| Android | Android Keystore | Archivo privado de la app |
| macOS (futuro) | Keychain | Archivo con permisos restrictivos |

- La clave privada no se registra en logs, no se incluye en JSON de IPC y no se envía por HTTP.
- El fallback debe documentar que un usuario con control del sistema puede copiarla.
- TPM/Keystore fuerte es opcional; no se exige para no excluir equipos legítimos.

## 4. Confianza inicial y distribución

- El POS incluye la clave pública de licencias en el binario o en un recurso firmado; no confía en una clave descargada al primer uso sin anclaje.
- La primera vinculación confía en Platform a través de TLS y de la aprobación del owner.
- El canal de aprobación (navegador) no recibe material privado.
- La clave pública del dispositivo se registra en Platform durante el enrolamiento.

## 5. Rotación y revocación

| Evento | Acción |
|---|---|
| Reinstalación con clave nueva | Nuevo `installation_identity` con `generation + 1`; el anterior se marca `retired_at`. |
| Pérdida de clave | Rehost con nueva clave y cierre del binding anterior. |
| Sospecha de compromiso | Revocar identidad, cerrar binding y evento `identity_revoked`. |
| Rotación de clave de emisión | Publicar nueva clave pública, mantener la anterior para grants vigentes, documentar ventana. |
| Revocación de licencia | Fuera de G4.2-A; requiere diseño de licenciamiento. |

Invariantes:

- Una clave activa pertenece a una sola instalación.
- Una instalación activa tiene una sola clave activa.
- Las claves retiradas no se reutilizan.

Límite de clonación: si dos equipos comparten clave **y** `installation_id`, son criptográficamente indistinguibles; `generation` y las señales de reconexión ayudan a detectar anomalías, pero no prueban unicidad física.

## 6. Separación de entornos

- Claves distintas por entorno (`development`, `staging`, `production`).
- La clave pública embebida debe identificar el entorno y rechazar grants de otro.
- Nunca se usan claves de desarrollo en producción ni viceversa.
- Las claves privadas de producción no se versionan ni se copian a repositorios.

## 7. Compatibilidad con grants existentes

- `LicenseGrant` v1 sigue siendo la forma canónica: `version`, `grant_id`, `installation_id`, `organization_id`, `plan`, `features`, `issued_at`, `valid_until`.
- No se añade la clave pública del dispositivo al grant v1.
- Si en el futuro se requiere vincular el grant a la clave del dispositivo, se hará con `LicenseGrant` v2 y verificación dual.
- El POS actual verifica v1; una Platform nueva debe seguir emitiendo v1 hasta que la compatibilidad esté probada.

## 8. Protección en disco y operación offline

- La clave privada se guarda fuera de SQLite, en el almacén del SO o en un archivo con permisos restrictivos.
- El backup del POS no debe incluir la clave privada en claro por defecto (PENDIENTE).
- La verificación de grants es offline y solo requiere la clave pública.
- El estado comercial se persiste en `local_installation_state`; no se necesita Platform por venta.
- Un reloj manipulado no invalida la firma, pero sí puede alterar la percepción de vigencia; el riesgo es aceptado y documentado.

## 9. Riesgos y decisiones pendientes

| Riesgo | Mitigación | Residual |
|---|---|---|
| Robo de clave privada del dispositivo | Keystore/ACL, detección de clave duplicada | Medio |
| Compromiso de la clave de emisión | Custodia offline, rotación | Alto si ocurre; debe existir plan |
| Reutilización de clave tras revocación | `retired_at` e índices únicos | Bajo |
| Grants de otro entorno | `key_id`/entorno y rechazo | Bajo |
| Restauración de clave en otro equipo | Detección de duplicado | Medio |

Decisiones PENDIENTES: custodia por plataforma, formato de `key_id`, ventana de rotación, política de backup de clave privada, y si se adopta Keystore fuerte como recomendación por defecto.

## 10. Referencias

- [ADR-0003](adr-0003-installation-enrollment.md)
- [Protocolo](g42-enrollment-protocol.md)
- [Modelo de datos](g42-data-model.md)
- POS `backend-rust/crates/pos-licensing/src/lib.rs` (solo lectura).
