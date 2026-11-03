use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signer, SigningKey};
use platform_application::LicenseSigner;

pub const DEV_SIGNING_SEED: [u8; 32] = [7u8; 32];

#[derive(Debug, Clone)]
pub struct Ed25519Signer {
    signing_key: SigningKey,
}

impl Ed25519Signer {
    pub fn from_seed_bytes(seed: [u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn from_seed_base64(value: &str) -> Result<Self, String> {
        let bytes = B64
            .decode(value.trim())
            .map_err(|_| "clave inválida".to_owned())?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| "clave inválida".to_owned())?;
        Ok(Self::from_seed_bytes(array))
    }

    pub fn public_key_b64(&self) -> String {
        B64.encode(self.signing_key.verifying_key().to_bytes())
    }

    pub fn sign_bytes(&self, message: &[u8]) -> Vec<u8> {
        self.signing_key.sign(message).to_bytes().to_vec()
    }
}

impl LicenseSigner for Ed25519Signer {
    fn sign(&self, grant_json: &str) -> platform_application::AppResult<String> {
        let signature = self.signing_key.sign(grant_json.as_bytes()).to_bytes();
        Ok(B64.encode(signature))
    }

    fn public_key_b64(&self) -> String {
        self.public_key_b64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firma_y_verifica_con_ed25519() {
        use ed25519_dalek::{Verifier, VerifyingKey};
        let signer = Ed25519Signer::from_seed_bytes(DEV_SIGNING_SEED);
        let message = "mensaje de prueba";
        let signature_b64 = signer.sign(message).unwrap();
        let bytes = B64.decode(signature_b64).unwrap();
        let signature = ed25519_dalek::Signature::from_slice(&bytes).unwrap();
        let verifying =
            VerifyingKey::from_bytes(&signer.signing_key.verifying_key().to_bytes()).unwrap();
        verifying.verify(message.as_bytes(), &signature).unwrap();
        assert!(!signer.public_key_b64().is_empty());
    }
}
