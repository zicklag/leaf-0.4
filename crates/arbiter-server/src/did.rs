//! DID web identity and signing key management for the arbiter server.
//!
//! Uses `atproto-identity` for key generation, JWT signing, and DID resolution.
//! The server generates a K-256 (secp256k1) key pair, serves a DID document
//! at `/.well-known/did.json`, and signs JWTs for inter-arbiter authentication.

use std::fs;
use std::path::Path;

use atproto_identity::key::{self, KeyData, KeyType};
use atproto_identity::model::{Document, DocumentBuilder};
use serde_json::json;

// ---------------------------------------------------------------------------
// DID identity
// ---------------------------------------------------------------------------

/// The server's DID identity, holding the signing key and DID string.
pub struct Identity {
    /// The server's DID web string, e.g. `did:web:localhost%3A3001`.
    pub did: String,
    /// The private key data for signing.
    pub key_data: KeyData,
}

impl Identity {
    /// Create a new identity with a freshly generated K-256 key pair.
    pub fn generate(did: String) -> Self {
        let key_data = key::generate_key(KeyType::K256Private)
            .expect("Failed to generate K-256 key");
        tracing::info!("Generated new K-256 key pair for {did}");
        Self { did, key_data }
    }

    /// Load an existing signing key from a hex-encoded file, or generate new if not found.
    pub fn load_or_generate(did: String, key_path: &Path) -> Self {
        if key_path.exists() {
            match fs::read_to_string(key_path) {
                Ok(hex) => {
                    let hex = hex.trim();
                    match hex::decode(hex) {
                        Ok(bytes) => {
                            let key_data = KeyData::new(KeyType::K256Private, bytes);
                            tracing::info!("Loaded signing key from {:?}", key_path);
                            return Self { did, key_data };
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse signing key: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read signing key: {e}");
                }
            }
        }

        let identity = Self::generate(did);

        if let Some(parent) = key_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let hex = hex::encode(identity.key_data.bytes());
        if let Err(e) = fs::write(key_path, &hex) {
            tracing::warn!("Failed to save signing key: {e}");
        } else {
            tracing::info!("Saved signing key to {:?}", key_path);
        }

        identity
    }

    /// Sign a JWT payload, returning the encoded JWT string.
    /// Uses ES256K algorithm (ECDSA with secp256k1).
    pub fn sign_jwt(&self, payload: &serde_json::Value) -> Result<String, String> {
        let header = json!({"alg": "ES256K", "typ": "JWT"});

        let header_b64 = encode_b64url(
            &serde_json::to_vec(&header).map_err(|e| format!("Header error: {e}"))?,
        );
        let payload_b64 = encode_b64url(
            &serde_json::to_vec(payload).map_err(|e| format!("Payload error: {e}"))?,
        );

        let message = format!("{header_b64}.{payload_b64}");
        let signature = key::sign(&self.key_data, message.as_bytes())
            .map_err(|e| format!("Signing error: {e}"))?;
        let sig_b64 = encode_b64url(&signature);

        Ok(format!("{message}.{sig_b64}"))
    }

    /// Build the DID document for this server.
    pub fn did_document(&self) -> Result<Document, String> {
        let public_key = key::to_public(&self.key_data)
            .map_err(|e| format!("Failed to derive public key: {e}"))?;
        let pub_key_bytes = public_key.bytes();
        let multibase = format!("z{}", bs58::encode(pub_key_bytes).into_string());

        let vm_id = format!("{}#atproto", self.did);

        DocumentBuilder::new()
            .add_context("https://www.w3.org/ns/did/v1")
            .add_context("https://w3id.org/security/multikey/v1")
            .id(self.did.clone())
            .add_multikey(vm_id.clone(), self.did.clone(), multibase)
            .add_extra(
                "authentication",
                json!([vm_id.clone()]),
            )
            .add_extra(
                "assertionMethod",
                json!([vm_id]),
            )
            .build()
            .map_err(|e| format!("Failed to build DID document: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Base64url-encode bytes without padding.
fn encode_b64url(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}
