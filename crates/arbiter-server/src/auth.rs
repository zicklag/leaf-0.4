//! Auth middleware for the arbiter XRPC server.
//!
//! Extracts the caller DID from:
//! 1. `Authorization: Bearer <JWT>` — verifies the JWT signature against
//!    the issuer's resolved DID document using atproto-identity.
//! 2. `Authorization: Bearer <token>` — with the unsafe dev token (if configured).

use std::sync::Arc;

use atproto_identity::resolve::IdentityResolver;
use atproto_oauth::jwt;
use base64::Engine as _;
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
    // Decode claims to get the issuer
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid JWT format: expected 3 parts");
    }

    let claims_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| anyhow::anyhow!("Failed to decode JWT claims: {e}"))?;

    let claims: jwt::Claims = serde_json::from_slice(&claims_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse JWT claims: {e}"))?;

    let issuer = claims
        .jose
        .issuer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No issuer in JWT claims"))?;

    // Resolve the DID document
    let did_document = identity_resolver
        .resolve(issuer)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to resolve DID {issuer}: {e}"))?;

    // Extract public keys from the DID document
    let did_keys = did_document.did_keys();
    if did_keys.is_empty() {
        anyhow::bail!("No verification keys in DID document for {issuer}");
    }

    // Try to verify the JWT signature with each key
    for key_multibase in did_keys {
        match atproto_identity::key::identify_key(key_multibase) {
            Ok(key_data) => {
                match jwt::verify(token, &key_data) {
                    Ok(_validated_claims) => {
                        return Ok(issuer.clone());
                    }
                    Err(_) => continue,
                }
            }
            Err(_) => continue,
        }
    }

    anyhow::bail!(
        "JWT signature could not be verified with any key for {issuer}"
    );
}
