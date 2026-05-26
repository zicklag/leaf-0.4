//! Auth middleware for the arbiter XRPC server.
//!
//! Extracts the caller DID from:
//! 1. `X-Caller-Did` header (dev mode)
//! 2. `Authorization: Bearer <token>` with unsafe dev token
//! 3. Body field `callerDid` (fallback)

use std::sync::Arc;

use salvo::prelude::*;

/// Auth configuration.
pub struct AuthConfig {
    unsafe_token: Option<String>,
}

impl AuthConfig {
    pub fn new() -> Self {
        Self { unsafe_token: None }
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

impl Default for AuthConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Auth middleware that extracts the caller DID.
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
        // 1. Check X-Caller-Did header (simple dev mode)
        let caller_did = req
            .header::<&str>("x-caller-did")
            .map(|s| s.to_string())
            .or_else(|| {
                // 2. Check Authorization header with unsafe token
                if let Some(auth) = req.header::<&str>("authorization")
                    && let Some(token) = auth.strip_prefix("Bearer ")
                    && let Some(ref unsafe_token) = self.config.unsafe_token
                    && token == unsafe_token
                {
                    return Some(token.to_string());
                }
                None
            })
            .unwrap_or_default();

        depot.insert("caller_did", caller_did);
        ctrl.call_next(req, depot, res).await;
    }
}
