//! Authentication middleware for the arbiter server.
//!
//! Uses `atproto-identity` for DID resolution and `atproto-oauth::jwt` for
//! JWT verification. Supports dev mode with an unsafe token bypass.

use std::sync::Arc;

use salvo::http::{StatusError, StatusCode, header};
use salvo::{
    Depot, FlowCtrl, Handler, Request, Response,
    async_trait,
};
use salvo::writing::Json;

use atproto_identity::key::{KeyType, KeyData};
use atproto_identity::resolve::InnerIdentityResolver;
use atproto_identity::model::VerificationMethod;
use atproto_oauth::encoding::FromBase64;
use atproto_oauth::jwt::{self, JoseClaims};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AuthConfig {
    pub unsafe_auth_token: Option<String>,
    pub server_did: String,
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
// Authenticated user
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub String);

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

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
// Token verification
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

    // Dev mode bypass
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

/// Verify an ES256K JWT using atproto-oauth.
///
/// Flow:
/// 1. Decode the JWT payload (base64url) to find the `iss` claim
/// 2. Resolve the issuer's DID document via atproto-identity
/// 3. Extract the #atproto Multikey public key
/// 4. Delegate full JWT verification to `atproto_oauth::jwt::verify`
async fn verify_jwt(token: &str, config: &AuthConfig) -> Result<String, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".to_string());
    }

    // Use FromBase64 to decode the JWT payload and extract issuer
    let jose_claims = JoseClaims::from_base64(parts[1])
        .map_err(|e| format!("Failed to decode JWT payload: {e}"))?;

    let issuer = jose_claims
        .issuer
        .ok_or_else(|| "JWT missing 'iss' claim".to_string())?;

    // Resolve the issuer's DID document
    let doc = config
        .resolver
        .resolve(&issuer)
        .await
        .map_err(|e| format!("Failed to resolve issuer DID {issuer}: {e}"))?;

    // Extract the #atproto verification key
    let pub_key_data = extract_atproto_key(&doc, &issuer)?;

    // Full JWT verification via atproto-oauth
    let claims = jwt::verify(token, &pub_key_data)
        .map_err(|e| format!("JWT verification failed: {e}"))?;

    let verified_issuer = claims
        .jose
        .issuer
        .unwrap_or(issuer);

    tracing::debug!("JWT verified for issuer: {verified_issuer}");
    Ok(verified_issuer)
}

/// Extract the #atproto Multikey from a DID document.
fn extract_atproto_key(doc: &atproto_identity::model::Document, did: &str) -> Result<KeyData, String> {
    let pub_key_mb = doc
        .verification_method
        .iter()
        .find(|vm| match vm {
            VerificationMethod::Multikey { id, .. } => id.ends_with("#atproto"),
            _ => false,
        })
        .and_then(|vm| match vm {
            VerificationMethod::Multikey { public_key_multibase, .. } => {
                Some(public_key_multibase.clone())
            }
            _ => None,
        })
        .ok_or_else(|| format!("No #atproto Multikey in DID document for {did}"))?;

    let encoded = pub_key_mb
        .strip_prefix('z')
        .ok_or_else(|| "Expected base58btc multibase (z prefix)".to_string())?;

    let bytes = bs58::decode(encoded)
        .into_vec()
        .map_err(|e| format!("Base58 decode error: {e}"))?;

    Ok(KeyData::new(KeyType::K256Public, bytes))
}

// ---------------------------------------------------------------------------
// Depot helper
// ---------------------------------------------------------------------------

pub fn get_authenticated_user(depot: &Depot) -> Result<&str, StatusError> {
    depot
        .get::<AuthenticatedUser>("user")
        .map(|u| u.0.as_str())
        .map_err(|_| StatusError::unauthorized())
}
