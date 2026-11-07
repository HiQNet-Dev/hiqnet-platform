//! Hashing de contraseñas (Argon2id) y tokens de sesión opacos.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use platform_core::auth::AuthError;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Parámetros Argon2id por defecto de la crate: m=19456 KiB, t=2, p=1, v19.
///
/// Coinciden con la recomendación base de OWASP para Argon2id y se validaron
/// localmente midiendo el tiempo de hash/verificación en el hardware de CI.
pub const ARGON2_MEMORY_KIB: u32 = 19_456;
pub const ARGON2_ITERATIONS: u32 = 2;
pub const ARGON2_PARALLELISM: u32 = 1;

pub fn argon2() -> Argon2<'static> {
    Argon2::default()
}

pub fn hash_password_sync(password: &str) -> Result<String, AuthError> {
    // La crate genera un salt aleatorio único por hash.
    argon2()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Storage)
}

pub async fn hash_password(password: String) -> Result<String, AuthError> {
    // Argon2 consume CPU y memoria: nunca bloquear el runtime async.
    tokio::task::spawn_blocking(move || hash_password_sync(&password))
        .await
        .map_err(|_| AuthError::Storage)?
}

pub fn verify_password_sync(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };
    argon2()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password_sync(&password, &hash))
        .await
        .unwrap_or(false)
}

/// Verificación contra un hash señuelo para igualar tiempos ante cuentas inexistentes.
pub async fn verify_dummy(password: String) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = tokio::task::spawn_blocking(|| {
        DUMMY
            .get_or_init(|| {
                hash_password_sync("hash-señuelo-sin-uso-real").unwrap_or_else(|_| String::new())
            })
            .clone()
    })
    .await
    .unwrap_or_default();
    let _ = verify_password(password, hash).await;
}

/// Genera 32 bytes aleatorios y devuelve token base64url sin relleno y su hash.
pub fn generate_session_token() -> (String, Vec<u8>) {
    use base64::Engine as _;
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2id_acepta_la_correcta_y_rechaza_la_incorrecta() {
        let hash = hash_password_sync("contraseña-de-prueba").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password_sync("contraseña-de-prueba", &hash));
        assert!(!verify_password_sync("otra", &hash));
        assert!(!verify_password_sync("x", "hash-invalido"));
    }

    /// Medición informativa, no un gate de CI: el hardware y la carga varían.
    /// Ejecutar con `cargo test -p platform-application benchmark_argon2id -- --ignored --nocapture`.
    #[test]
    #[ignore = "medición de rendimiento informativa"]
    fn benchmark_argon2id() {
        let iterations: u32 = std::env::var("ARGON2_BENCH_ITERATIONS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            let hash = hash_password_sync("medición-de-costo-local").unwrap();
            assert!(verify_password_sync("medición-de-costo-local", &hash));
        }
        let average = start.elapsed() / iterations.max(1);
        println!(
            "Argon2id (m={ARGON2_MEMORY_KIB} KiB, t={ARGON2_ITERATIONS}, p={ARGON2_PARALLELISM}): \
             promedio hash+verify {average:?} ({iterations} iteraciones)"
        );
    }

    #[test]
    fn tokens_son_aleatorios_y_su_hash_es_estable() {
        let (a, hash_a) = generate_session_token();
        let (b, hash_b) = generate_session_token();
        assert_ne!(a, b);
        assert_ne!(hash_a, hash_b);
        assert_eq!(hash_a, hash_token(&a));
        assert!(!a.contains('='));
    }
}
