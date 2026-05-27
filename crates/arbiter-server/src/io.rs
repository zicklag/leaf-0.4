//! Remote arbiter resolution IO layer.
//!
//! Provides HTTP-based resolution of remote space members for the
//! arbiter core's suspension/resume loop.

use serde_json::Value;

/// Resolves a remote XRPC query by fetching data from the remote arbiter.
pub async fn resolve_remote(
    client: &reqwest::Client,
    remote_did: &str,
    path: &str,
    _input: &Value,
) -> Value {
    // Build the remote URL from the DID
    let base_url = if let Some(host) = remote_did.strip_prefix("did:web:") {
        let host = host.replace("%3A", ":");
        format!("https://{host}")
    } else {
        tracing::warn!(%remote_did, "Cannot resolve non-web DID yet");
        return Value::Array(vec![]);
    };

    let url = format!("{base_url}{path}");

    match client.get(&url).send().await {
        Ok(resp) => {
            if let Ok(body) = resp.json::<Value>().await {
                body.get("members").cloned().unwrap_or(Value::Array(vec![]))
            } else {
                Value::Array(vec![])
            }
        }
        Err(e) => {
            tracing::warn!(%remote_did, %e, "Failed to fetch remote data");
            Value::Array(vec![])
        }
    }
}
