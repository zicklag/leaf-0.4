//! PLC directory HTTP client.
//!
//! Handles submission of genesis and update operations to a PLC directory
//! server (e.g., a local instance at `http://localhost:3001`).

use std::sync::Arc;

use anyhow::Context;
use atproto_plc::{Operation, PlcState};
use tokio::sync::Mutex;

use crate::ServerState;

/// Keys and latest operation CID for a DID managed by this server.
#[derive(Clone)]
pub struct DidState {
    pub keys: std::sync::Arc<atproto_plc::BuilderKeys>,
    /// CID of the most recent operation (genesis or last update).
    /// Used as `prev` when building the next update operation.
    pub latest_cid: Option<String>,
}

/// A store for the private keys and state associated with DIDs managed by this
/// server.
///
/// Keys are kept in-memory so they can sign future update operations.
/// Persistence will be added later.
pub type DidKeyStore = Arc<Mutex<std::collections::HashMap<String, DidState>>>;

/// Submit a signed genesis/update operation to the PLC directory.
///
/// Per the PLC spec, the operation is POSTed to `{directory_url}/{did}`.
pub async fn submit_operation(
    state: &ServerState,
    did: &str,
    operation: &Operation,
) -> anyhow::Result<()> {
    let url = format!("{}/{}", state.plc_directory_url.trim_end_matches('/'), did);

    let body = serde_json::to_value(operation)?;

    tracing::info!(%url, "Submitting operation to PLC directory");

    let resp = state
        .client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to contact PLC directory")?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "PLC directory returned {status} for {did}: {text}"
        );
    }

    tracing::info!(%did, "PLC operation accepted");
    Ok(())
}

/// Fetch the current PLC state for a DID.
///
/// Endpoint: `GET {directory_url}/{did}/data`
pub async fn fetch_state(
    state: &ServerState,
    did: &str,
) -> anyhow::Result<PlcState> {
    let url = format!(
        "{}/{did}/data",
        state.plc_directory_url.trim_end_matches('/')
    );

    let resp = state
        .client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch PLC state")?;

    if !resp.status().is_success() {
        anyhow::bail!("PLC directory returned {} for {did}/data", resp.status());
    }

    let plc_state = resp
        .json::<PlcState>()
        .await
        .context("Failed to parse PLC state response")?;

    Ok(plc_state)
}

/// Fetch the current PLC state AND the latest operation CID from the audit log.
///
/// Uses `/log/audit` to find the most recent valid operation's CID, which
/// is needed as `prev` when building an update operation.
pub async fn fetch_state_with_cid(
    state: &ServerState,
    did: &str,
) -> anyhow::Result<(PlcState, String)> {
    // First get the current state
    let plc_state = fetch_state(state, did).await?;

    // Then get the latest operation CID from the audit log
    let log_url = format!(
        "{}/{did}/log/audit",
        state.plc_directory_url.trim_end_matches('/')
    );

    let resp = state
        .client
        .get(&log_url)
        .send()
        .await
        .context("Failed to fetch PLC audit log")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "PLC directory returned {} for {did}/log/audit",
            resp.status()
        );
    }

    // The audit log returns an array of operations; we need the last valid one's CID.
    let log: Vec<serde_json::Value> = resp
        .json()
        .await
        .context("Failed to parse PLC audit log")?;

    let latest_cid = log
        .last()
        .and_then(|entry| entry.get("cid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("No operations found in audit log for {did}"))?;

    Ok((plc_state, latest_cid))
}
