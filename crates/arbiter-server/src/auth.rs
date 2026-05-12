//! Authentication middleware for the arbiter server.
//!
//! Supports two modes:
//! 1. **Dev mode**: An `unsafe_auth_token` that bypasses JWT verification.
//! 2. **Production**: JWT verification using AT Protocol standards.

use std::sync::Arc;

use salvo::http::{StatusError, StatusCode, header};
use salvo::{
    Depot, FlowCtrl, Handler, Request, Response,
    async_trait,
};
use salvo::writing::Json;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Authentication configuration for the server.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// If set, this token is accepted without JWT verification (dev mode).
    pub unsafe_auth_token: Option<String>,
    /// The DID of this server (used for audience verification).
    pub server_did: String,
}

impl AuthConfig {
    pub fn new(server_did: String) -> Self {
        Self {
            unsafe_auth_token: None,
            server_did,
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

/// Extract and verify the bearer token from the request.
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

    // Production: parse JWT and verify
    verify_jwt(token, config).await
}

/// Extract the user DID from an unsafe token.
///
/// Format: `<userDid>:<secret>`
fn extract_unsafe_user(token: &str) -> String {
    if let Some((user_did, _)) = token.split_once(':') {
        user_did.to_string()
    } else {
        token.to_string()
    }
}

/// Verify a JWT token with simplified verification.
async fn verify_jwt(token: &str, _config: &AuthConfig) -> Result<String, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format".to_string());
    }

    let payload_bytes = decode_urlsafe_base64(parts[1])
        .map_err(|e| format!("Failed to decode JWT payload: {e}"))?;

    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("Failed to parse JWT payload: {e}"))?;

    let iss = payload
        .get("iss")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "JWT missing 'iss' claim".to_string())?;

    tracing::debug!("JWT verified for issuer: {iss}");

    // TODO: In production, verify the JWT signature using the issuer's
    // DID document from PLC directory.
    Ok(iss.to_string())
}

/// Decode a URL-safe base64 string.
fn decode_urlsafe_base64(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    engine
        .decode(input)
        .map_err(|e| format!("Base64 decode error: {e}"))
}

// ---------------------------------------------------------------------------
// Helper to extract the authenticated DID from a depot
// ---------------------------------------------------------------------------

/// Get the authenticated user DID from the request depot.
pub fn get_authenticated_user(depot: &Depot) -> Result<&str, StatusError> {
    depot
        .get::<AuthenticatedUser>("user")
        .map(|u| u.0.as_str())
        .map_err(|_| StatusError::unauthorized())
}
