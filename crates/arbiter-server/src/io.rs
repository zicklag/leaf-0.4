//! HTTP-based `ArbiterIo` implementation for the arbiter server.
//!
//! Uses `atproto-identity` for DID resolution and handles remote space member
//! resolution by making HTTP requests to other arbiter servers.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;

use atproto_identity::resolve::InnerIdentityResolver;

use arbiter_core::ResolvedMemberList;
use arbiter_core::futures::ArbiterIo;

use crate::did::Identity;

// ---------------------------------------------------------------------------
// HttpArbiterIo
// ---------------------------------------------------------------------------

/// `ArbiterIo` implementation using HTTP to resolve remote spaces.
pub struct HttpArbiterIo {
    client: Client,
    /// Resolves DIDs to their documents (plc + web).
    resolver: Arc<InnerIdentityResolver>,
    /// Server's own Identity for JWT signing.
    identity: Arc<Identity>,
}

impl HttpArbiterIo {
    pub fn new(
        client: Client,
        resolver: Arc<InnerIdentityResolver>,
        identity: Arc<Identity>,
    ) -> Self {
        Self {
            client,
            resolver,
            identity,
        }
    }

    fn create_auth_token(&self, audience_did: &str) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs() as i64;

        let payload = serde_json::json!({
            "iss": self.identity.did,
            "aud": audience_did,
            "exp": now + 60,
            "iat": now,
        });

        self.identity.sign_jwt(&payload)
    }
}

#[async_trait]
impl ArbiterIo for HttpArbiterIo {
    async fn resolve_remote_members(
        &self,
        arbiter_did: &str,
        space_key: &str,
        _resolver_depth: i64,
    ) -> Result<ResolvedMemberList, String> {
        let doc = self
            .resolver
            .resolve(arbiter_did)
            .await
            .map_err(|e| format!("Failed to resolve DID {arbiter_did}: {e}"))?;

        let base_url = doc
            .service
            .iter()
            .find(|s| {
                s.r#type == "MuniArbiter"
                    || s.r#type == "ArbiterServer"
                    || s.r#type == "AtprotoPersonalDataServer"
            })
            .map(|s| s.service_endpoint.clone())
            .or_else(|| doc.service.first().map(|s| s.service_endpoint.clone()))
            .ok_or_else(|| {
                format!("No service endpoint found in DID document for {arbiter_did}")
            })?;

        let jwt = self.create_auth_token(arbiter_did)?;
        let base_url = base_url.trim_end_matches('/');
        let url = format!("{base_url}/xrpc/town.muni.arbiter.resolveMembers");

        let response = self
            .client
            .get(&url)
            .query(&[("spaceKey", space_key)])
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Remote server returned {status}: {body}"));
        }

        response
            .json::<ResolvedMemberList>()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))
    }
}
