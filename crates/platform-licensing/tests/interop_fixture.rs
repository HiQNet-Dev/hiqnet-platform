//! Fixture de interoperabilidad Platform → POS.
//!
//! El documento se genera con las primitivas reales de emisión de Platform
//! (`LicenseGrant::to_canonical_json` + `Ed25519Signer`) y datos exclusivamente
//! sintéticos: semilla de desarrollo `[7u8; 32]` (nunca usada en instaladores)
//! e identificadores fijos de prueba. El mismo fichero vive en ambos
//! repositorios:
//!
//! - Platform: `crates/platform-licensing/tests/fixtures/platform-grant-v1.json`
//! - POS:      `backend-rust/crates/pos-storage-sqlite/tests/fixtures/platform-grant-v1.json`
//!
//! El POS lo instala tal cual (sin reconstruir ni volver a firmar) para probar
//! la interoperabilidad real de ambas implementaciones. Este test regenera el
//! documento con las primitivas actuales y lo compara byte a byte: cualquier
//! drift del contrato v1 (orden de campos, representación de fechas, firma)
//! rompe aquí antes de llegar al POS.
//!
//! Regenerar el fixture tras un cambio consciente del contrato:
//! `HIQNET_REGENERAR_FIXTURE=1 cargo test -p platform-licensing --test interop_fixture`

use platform_application::LicenseSigner as _;
use platform_core::ids::{GrantId, InstallationId, OrganizationId};
use platform_core::license::{LICENSE_DOC_VERSION, LicenseGrant};
use platform_licensing::{DEV_SIGNING_SEED, Ed25519Signer};
use time::OffsetDateTime;
use uuid::Uuid;

const FIXTURE_PATH: &str = "tests/fixtures/platform-grant-v1.json";

fn documento() -> (String, String, String) {
    let signer = Ed25519Signer::from_seed_bytes(DEV_SIGNING_SEED);
    let grant = LicenseGrant {
        version: LICENSE_DOC_VERSION,
        grant_id: GrantId::from_uuid(
            Uuid::parse_str("c0ffee00-0000-4000-8000-000000000001").unwrap(),
        ),
        installation_id: InstallationId::from_uuid(
            Uuid::parse_str("0f9e8d7c-0000-4000-8000-0000000000bb").unwrap(),
        ),
        organization_id: OrganizationId::from_uuid(
            Uuid::parse_str("0f9e8d7c-0000-4000-8000-0000000000aa").unwrap(),
        ),
        plan: "pro".to_owned(),
        features: vec![
            "catalog".to_owned(),
            "sales".to_owned(),
            "backups.local".to_owned(),
        ],
        issued_at: OffsetDateTime::from_unix_timestamp(1_767_225_600).unwrap(),
        // Vigencia lejana y fija: los bytes del fixture son deterministas y la
        // prueba de interoperabilidad exige vigencia futura en cualquier fecha.
        valid_until: OffsetDateTime::from_unix_timestamp(4_070_908_800).unwrap(),
    };
    let grant_json =
        String::from_utf8(grant.to_canonical_json().expect("canonical json")).expect("utf8");
    let signature_b64 = signer.sign(&grant_json).expect("firma");
    let public_key_b64 = signer.public_key_b64();
    (grant_json, signature_b64, public_key_b64)
}

#[test]
fn fixture_coincide_con_la_implementacion_real() {
    let (grant_json, signature_b64, public_key_b64) = documento();
    let esperado = serde_json::json!({
        "grant_json": grant_json,
        "signature_b64": signature_b64,
        "public_key_b64": public_key_b64,
    });

    if std::env::var("HIQNET_REGENERAR_FIXTURE").ok().as_deref() == Some("1") {
        std::fs::write(
            FIXTURE_PATH,
            serde_json::to_string_pretty(&esperado).unwrap() + "\n",
        )
        .expect("escribir fixture");
    }

    let committed = std::fs::read_to_string(FIXTURE_PATH).expect("fixture presente");
    let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(
        committed, esperado,
        "drift del contrato LicenseGrant v1: el fixture y la implementación real difieren"
    );
}

#[test]
fn la_semilla_del_fixture_es_sintetica() {
    // El par del fixture es exclusivamente de pruebas: la semilla DEV nunca se
    // usa en instaladores ni en producción.
    assert_eq!(DEV_SIGNING_SEED, [7u8; 32]);
}
