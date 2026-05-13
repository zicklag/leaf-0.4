//! DID web identity and signing key management for the arbiter server.
//!
//! Uses `atproto-identity` for key generation and `atproto-oauth::jwt` for JWT minting.

use std::fs;
use std::path::Path;

use atproto_identity::key::{self, KeyData, KeyType};
use atproto_identity::model::{Document, DocumentBuilder};
use atproto_oauth::jwt::{self, Header, Claims, JoseClaims};

// ---------------------------------------------------------------------------
// DID identity
// ---------------------------------------------------------------------------

/// The server's DID identity, holding the signing key and DID string.
pub struct Identity {
    pub did: String,
    pub key_data: KeyData,
}

impl Identity {
    pub fn generate(did: String) -> Self {
        let key_data = key::generate_key(KeyType::K256Private)
            .expect("Failed to generate K-256 key");
        tracing::info!("Generated new K-256 key pair for {did}");
        Self { did, key_data }
    }

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
                        Err(e) => tracing::warn!("Failed to parse signing key: {e}"),
                    }
                }
                Err(e) => tracing::warn!("Failed to read signing key: {e}"),
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

    /// Sign a JWT with ES256K using the server's private key.
    pub fn sign_jwt(&self, audience_did: &str) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs();

        let header = Header {
            algorithm: Some("ES256K".to_string()),
            type_: Some("JWT".to_string()),
            ..Default::default()
        };

        let claims = Claims::new(JoseClaims {
            issuer: Some(self.did.clone()),
            audience: Some(audience_did.to_string()),
            expiration: Some(now + 60),
            issued_at: Some(now),
            ..Default::default()
        });

        jwt::mint(&self.key_data, &header, &claims)
            .map_err(|e| format!("JWT minting error: {e}"))
    }

    /// Build the DID document for `/.well-known/did.json`.
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
            .add_extra("authentication", serde_json::json!([vm_id.clone()]))
            .add_extra("assertionMethod", serde_json::json!([vm_id]))
            .build()
            .map_err(|e| format!("Failed to build DID document: {e}"))
    }
}
