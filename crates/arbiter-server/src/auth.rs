//! Auth middleware for the arbiter XRPC server.
//!
//! Extracts the caller DID from:
//! 1. `Authorization: Bearer <JWT>` — verifies the JWT signature against
//!    the issuer's resolved DID document using atproto-identity.
//! 2. `Authorization: Bearer <token>` — with the unsafe dev token (if configured).

use std::sync::Arc;

use atproto_identity::key;
use atproto_identity::resolve::IdentityResolver;
use atproto_oauth::encoding::FromBase64;
use atproto_oauth::jwt;
use salvo::prelude::*;

/// Auth configuration.
pub struct AuthConfig {
    unsafe_token: Option<String>,
    identity_resolver: Arc<dyn IdentityResolver>,
}

impl AuthConfig {
    /// Create a new auth config with the given identity resolver.
    pub fn new(identity_resolver: Arc<dyn IdentityResolver>) -> Self {
        Self {
            unsafe_token: None,
            identity_resolver,
        }
    }

    /// Enable an unsafe development token that bypasses real auth.
    pub fn with_unsafe_token(mut self, token: String) -> Self {
        self.unsafe_token = Some(token);
        self
    }

    pub fn with_unsafe_token_if(self, token: Option<String>) -> Self {
        match token {
            Some(t) => self.with_unsafe_token(t),
            None => self,
        }
    }
}

/// Auth middleware that extracts the caller DID from JWT tokens.
#[derive(Clone)]
pub struct AuthMiddleware {
    config: Arc<AuthConfig>,
}

impl AuthMiddleware {
    pub fn new(config: Arc<AuthConfig>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl salvo::Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        // Try to extract Authorization: Bearer <token>
        let auth_header = req
            .header::<&str>("authorization")
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        let caller_did = match auth_header {
            Some(token) => {
                // First check for unsafe dev token
                if let Some(ref unsafe_token) = self.config.unsafe_token
                    && token == *unsafe_token
                {
                    // Unsafe token matched — use the token value as the DID
                    // (the client sets their DID as the token for dev purposes)
                    token
                } else {
                    // Try JWT verification
                    match verify_jwt(&token, &*self.config.identity_resolver).await {
                        Ok(did) => did,
                        Err(e) => {
                            tracing::warn!(%e, "JWT verification failed");
                            String::new()
                        }
                    }
                }
            }
            None => String::new(),
        };

        depot.insert("caller_did", caller_did);
        ctrl.call_next(req, depot, res).await;
    }
}

/// Verify a JWT token and extract the issuer DID.
///
/// Steps:
/// 1. Decode claims to get the issuer DID
/// 2. Resolve the DID document via the identity resolver
/// 3. Extract public keys from the DID document
/// 4. Verify the JWT signature against each key
async fn verify_jwt(
    token: &str,
    identity_resolver: &dyn IdentityResolver,
) -> anyhow::Result<String> {
    let issuer = decode_jwt_issuer(token)?;

    // Resolve the DID document
    let did_document = identity_resolver
        .resolve(&issuer)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to resolve DID {issuer}: {e}"))?;

    // Extract public keys from the DID document
    let did_keys = did_document.did_keys();
    if did_keys.is_empty() {
        anyhow::bail!("No verification keys in DID document for {issuer}");
    }

    // Try to verify the JWT signature with each key
    for key_multibase in did_keys {
        let Ok(key_data) = key::identify_key(key_multibase) else {
            continue;
        };
        if jwt::verify(token, &key_data).is_ok() {
            return Ok(issuer);
        }
    }

    anyhow::bail!(
        "JWT signature could not be verified with any key for {issuer}"
    );
}

/// Extract the issuer DID from a JWT without full signature verification.
/// Only the claims payload is decoded (without verification) to determine
/// which DID to resolve. Actual signature verification happens afterward
/// via `jwt::verify` once the DID document's keys are obtained.
fn decode_jwt_issuer(token: &str) -> anyhow::Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Invalid JWT: expected 3 parts"))?;

    let claims = jwt::Claims::from_base64(payload)?;

    claims
        .jose
        .issuer
        .ok_or_else(|| anyhow::anyhow!("JWT missing required 'iss' claim"))
}
