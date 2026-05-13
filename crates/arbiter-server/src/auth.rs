//! Authentication middleware for the arbiter server.
//!
//! Supports two modes:
//! 1. **Dev mode**: An `unsafe_auth_token` that bypasses JWT verification.
//! 2. **Production**: ES256K JWT verification using `atproto-identity` for
//!    DID resolution and cryptographic validation.

use std::sync::Arc;

use salvo::http::{StatusError, StatusCode, header};
use salvo::{
    Depot, FlowCtrl, Handler, Request, Response,
    async_trait,
};
use salvo::writing::Json;

use atproto_identity::key::{self, KeyType, KeyData};
use atproto_identity::resolve::InnerIdentityResolver;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Authentication configuration for the server.
#[derive(Clone)]
pub struct AuthConfig {
    /// If set, this token is accepted without JWT verification (dev mode).
    pub unsafe_auth_token: Option<String>,
    /// The DID of this server (used for audience verification).
    pub server_did: String,
    /// DID resolver for fetching DID documents and extracting verification keys.
    pub resolver: Arc<InnerIdentityResolver>,
}

impl AuthConfig {
    pub fn new(server_did: String, resolver: Arc<InnerIdentityResolver>) -> Self {
        Self {
            unsafe_auth_token: None,
            server_did,
            resolver,
        }
    }

    pub fn with_unsafe_token(mut self, token: String) -> Self {
        self.unsafe_auth_token = Some(token);
        self
    }
}

// ---------------------------------------------------------------------------
// Authenticated user extractor
// ---------------------------------------------------------------------------

/// The authenticated user DID stored in the depot.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub String);

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

/// Salvo middleware that authenticates requests.
#[derive(Clone)]
pub struct AuthMiddleware {
    pub config: Arc<AuthConfig>,
}

impl AuthMiddleware {
    pub fn new(config: Arc<AuthConfig>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        match extract_and_verify_token(req, &self.config).await {
            Ok(did) => {
                depot.insert("user", AuthenticatedUser(did));
                ctrl.call_next(req, depot, res).await;
            }
            Err(e) => {
                tracing::warn!("Auth failed: {e}");
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(Json(serde_json::json!({
                    "error": "AuthenticationRequired",
                    "message": e,
                })));
                ctrl.skip_rest();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Token extraction and verification
// ---------------------------------------------------------------------------

async fn extract_and_verify_token(
    req: &Request,
    config: &AuthConfig,
) -> Result<String, String> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing Authorization header".to_string())?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| "Authorization header must use Bearer scheme".to_string())?;

    // Dev mode: unsafe token bypass
    if let Some(unsafe_token) = &config.unsafe_auth_token {
        if token == unsafe_token {
            return Ok(extract_unsafe_user(token));
        }
    }

    verify_jwt(token, config).await
}

fn extract_unsafe_user(token: &str) -> String {
    if let Some((user_did, _)) = token.split_once(':') {
        user_did.to_string()
    } else {
        token.to_string()
    }
}

/// Verify an ES256K JWT.
///
/// 1. Decode header/payload
/// 2. Extract the `iss` claim
/// 3. Resolve the issuer's DID document via atproto-identity
/// 4. Extract the #atproto verification key
/// 5. Verify the JWT signature
async fn verify_jwt(token: &str, config: &AuthConfig) -> Result<String, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".to_string());
    }

    // Decode header to check algorithm
    let header_bytes =
        decode_b64url(parts[0]).map_err(|e| format!("Failed to decode JWT header: {e}"))?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| format!("Failed to parse JWT header: {e}"))?;
    let alg = header
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "JWT missing 'alg'".to_string())?;

    if alg != "ES256K" {
        return Err(format!("Unsupported algorithm: {alg} (expected ES256K)"));
    }

    // Decode payload to extract issuer
    let payload_bytes =
        decode_b64url(parts[1]).map_err(|e| format!("Failed to decode JWT payload: {e}"))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("Failed to parse JWT payload: {e}"))?;

    let issuer = payload
        .get("iss")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "JWT missing 'iss' claim".to_string())?;

    // Resolve issuer's DID document
    let doc = config
        .resolver
        .resolve(issuer)
        .await
        .map_err(|e| format!("Failed to resolve issuer DID {issuer}: {e}"))?;

    // Extract the #atproto verification key
    let pub_key_mb = doc
        .verification_method
        .iter()
        .find(|vm| {
            use atproto_identity::model::VerificationMethod;
            match vm {
                VerificationMethod::Multikey { id, .. } => id.ends_with("#atproto"),
                _ => false,
            }
        })
        .and_then(|vm| match vm {
            atproto_identity::model::VerificationMethod::Multikey {
                public_key_multibase,
                ..
            } => Some(public_key_multibase.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            format!("No #atproto Multikey verification method in DID document for {issuer}")
        })?;

    // Decode the multibase public key
    let pub_key_bytes = decode_multibase(&pub_key_mb)?;
    let public_key = KeyData::new(KeyType::K256Public, pub_key_bytes);

    // Verify the signature
    let message = format!("{}.{}", parts[0], parts[1]);
    let signature = decode_b64url(parts[2])?;

    key::validate(&public_key, &signature, message.as_bytes())
        .map_err(|e| format!("JWT signature verification failed: {e}"))?;

    tracing::debug!("JWT verified for issuer: {issuer}");
    Ok(issuer.to_string())
}

// ---------------------------------------------------------------------------
// Base64url / multibase helpers
// ---------------------------------------------------------------------------

fn decode_b64url(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| format!("Base64 decode error: {e}"))
}

/// Decode a multibase-encoded key (base58btc with 'z' prefix).
fn decode_multibase(input: &str) -> Result<Vec<u8>, String> {
    let encoded = input
        .strip_prefix('z')
        .ok_or_else(|| "Expected base58btc multibase (z prefix)".to_string())?;
    bs58::decode(encoded)
        .into_vec()
        .map_err(|e| format!("Base58 decode error: {e}"))
}

// ---------------------------------------------------------------------------
// Helper to extract the authenticated DID from a depot
// ---------------------------------------------------------------------------

pub fn get_authenticated_user(depot: &Depot) -> Result<&str, StatusError> {
    depot
        .get::<AuthenticatedUser>("user")
        .map(|u| u.0.as_str())
        .map_err(|_| StatusError::unauthorized())
}
