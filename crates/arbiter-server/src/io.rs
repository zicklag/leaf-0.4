//! HTTP-based `ArbiterIo` implementation for the arbiter server.
//!
//! Handles remote space member resolution by making HTTP requests to other
//! arbiter servers.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;

use arbiter_core::ResolvedMemberList;
use arbiter_core::futures::ArbiterIo;

// ---------------------------------------------------------------------------
// DidResolver trait
// ---------------------------------------------------------------------------

/// Resolves a DID to its service endpoint URL.
#[async_trait]
pub trait DidResolver: Send + Sync {
    /// Resolve a DID to its service endpoint URL.
    async fn resolve(&self, did: &str) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// HttpArbiterIo
// ---------------------------------------------------------------------------

/// `ArbiterIo` implementation using HTTP to resolve remote spaces.
pub struct HttpArbiterIo {
    client: Client,
    /// Resolves arbiter DIDs to their service endpoint URLs.
    did_resolver: Arc<dyn DidResolver>,
    /// Bearer token for authenticating to remote arbiters.
    auth_token: String,
}

impl HttpArbiterIo {
    /// Create a new `HttpArbiterIo`.
    pub fn new(
        client: Client,
        did_resolver: Arc<dyn DidResolver>,
        auth_token: String,
    ) -> Self {
        Self {
            client,
            did_resolver,
            auth_token,
        }
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
        let base_url = self
            .did_resolver
            .resolve(arbiter_did)
            .await
            .map_err(|e| format!("Failed to resolve DID {arbiter_did}: {e}"))?;

        let url = format!("{base_url}/xrpc/town.muni.arbiter.resolveMembers");

        let response = self
            .client
            .get(&url)
            .query(&[("spaceKey", space_key)])
            .header("Authorization", format!("Bearer {}", self.auth_token))
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

// ---------------------------------------------------------------------------
// PlcDidResolver
// ---------------------------------------------------------------------------

/// Resolves arbiter DIDs to their service endpoints using PLC directory.
pub struct PlcDidResolver {
    /// The PLC directory base URL (e.g., "https://plc.directory").
    plc_url: String,
    /// HTTP client for DID document lookups.
    client: Client,
}

impl PlcDidResolver {
    /// Create a new `PlcDidResolver`.
    pub fn new(plc_url: String, client: Client) -> Self {
        Self { plc_url, client }
    }
}

#[async_trait]
impl DidResolver for PlcDidResolver {
    async fn resolve(&self, did: &str) -> Result<String, String> {
        // Look up the DID document from the PLC directory
        let url = format!("{}/{}", self.plc_url, did);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to resolve DID: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "PLC directory returned {} for DID {did}",
                response.status()
            ));
        }

        let doc: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse DID document: {e}"))?;

        // Extract the service endpoint from the DID document
        let services = doc
            .get("service")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "No service array in DID document".to_string())?;

        let arbiter_service = services
            .iter()
            .find(|s| {
                s.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "MuniArbiter" || t == "ArbiterServer")
                    .unwrap_or(false)
            })
            .ok_or_else(|| "No MuniArbiter service endpoint in DID document".to_string())?;

        let endpoint = arbiter_service
            .get("serviceEndpoint")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Service missing serviceEndpoint".to_string())?;

        Ok(endpoint.to_string())
    }
}
