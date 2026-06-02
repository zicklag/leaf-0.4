//! Single XRPC endpoint handler.
//!
//! All requests come through one handler.  The `atproto-proxy` header
//! identifies the target arbiter DID.  The handler feeds an
//! [`Event::IncomingXrpc`] to that arbiter's [`StateMachine`] and returns
//! the response.
//!
//! For `createArbiter`, the handler bootstraps a new arbiter state machine
//! backed by a PDS account (app password). Any XRPC requests triggered
//! during policy evaluation are proxied through that PDS.

use std::sync::Arc;

use arbiter_core::{
    Event, IoAction, NSID, XrpcResponse,
    nsid_method,
    policy_core::XrpcMethod,
};
use salvo::prelude::*;
use salvo::writing::Json;
use serde_json::Value;

use crate::state::{ArbiterCollection, PdsAccount};
use crate::ServerState;
use crate::state::Did;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn caller_did(depot: &Depot) -> String {
    depot
        .get::<String>("caller_did")
        .cloned()
        .unwrap_or_default()
}

fn err(msg: &str) -> Value {
    serde_json::json!({ "error": msg })
}

/// Extract the target arbiter DID from the `atproto-proxy` header.
///
/// Format: `{did}#{serviceId}`  e.g. `did:plc:abc123#arbiter`
fn arbiter_did_from_proxy(req: &Request) -> Option<String> {
    let header = req.header::<&str>("atproto-proxy")?;
    let did = header.split('#').next()?;
    if did.is_empty() {
        return None;
    }
    Some(did.to_string())
}

/// Extract the NSID from the XRPC path: `/xrpc/town.muni.arbiter.getArbiterConfig`
fn nsid_from_path(req: &Request) -> Option<String> {
    let path = req.uri().path();
    path.strip_prefix("/xrpc/")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn build_params(body: &Value) -> Value {
    let mut params = body.clone();
    if let Some(obj) = params.as_object_mut() {
        obj.remove("arbiterDid");
    }
    params
}

async fn parse_body_or_query(req: &mut Request) -> Value {
    if req.method() == salvo::http::Method::GET {
        let mut map = serde_json::Map::new();
        for key in &["spaceKey", "spaceType"] {
            if let Some(v) = req.query::<String>(key) {
                map.insert(key.to_string(), Value::String(v));
            }
        }
        Value::Object(map)
    } else {
        req.parse_json::<Value>().await.unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Request processing — drive an event through the state machine
// ---------------------------------------------------------------------------

/// Feed an [`Event`] to an arbiter's [`StateMachine`] and drive any remote
/// requests to completion, returning the final response.
///
/// If the arbiter has an associated PDS account (app-password arbiter),
/// remote XRPC requests are proxied through that PDS instead of being
/// resolved directly.
async fn process_request(
    coll: &mut ArbiterCollection,
    client: &reqwest::Client,
    did: &str,
    event: Event,
) -> (u16, Value) {
    let mut stack = vec![event];

    while let Some(event) = stack.pop() {
        let actions = match coll.arbiters.get_mut(did) {
            Some(sm) => sm.handle_event(event),
            None => {
                return (
                    404,
                    serde_json::json!({"error": "ErrArbiterNotExists"}),
                );
            }
        };

        for action in actions {
            match action {
                IoAction::SendXrpcResponse { body, status } => {
                    return (status, body);
                }
                IoAction::SendXrpcRequest {
                    did: target_did,
                    method,
                    nsid,
                    input,
                    job_id,
                } => {
                    // Check if this arbiter has a PDS account — if so,
                    // proxy the request through the PDS.
                    let pds_account = coll.get_pds_account(did)
                        .cloned()
                        .expect("every arbiter must have a PDS account");
                    let resp = resolve_remote_request(
                        client,
                        &pds_account,
                        &target_did,
                        &method,
                        &nsid,
                        input,
                    )
                    .await;
                    stack.push(Event::XrpcRemoteResult {
                        status: resp.status,
                        body: resp.body,
                        job_id,
                    });
                }
            }
        }
    }

    (
        500,
        serde_json::json!({"error": "ErrNoResponse"}),
    )
}

/// Proxy a remote XRPC request through the arbiter's PDS account.
///
/// The PDS URL is used as the base endpoint, the app password is
/// included in the `Authorization` header, and the target DID+service
/// fragment is passed in the `atproto-proxy` header.
async fn resolve_remote_request(
    client: &reqwest::Client,
    pds: &PdsAccount,
    remote_did: &str,
    method: &XrpcMethod,
    path: &str,
    input: Value,
) -> XrpcResponse {
    let url = format!(
        "{}/xrpc/{}",
        pds.pds_endpoint.trim_end_matches('/'),
        path.trim_start_matches('/'),
    );

    let proxy_header = format!("{remote_did}");

    let req_builder = match method {
        XrpcMethod::Query => {
            let mut req = client.get(&url);
            if let Some(obj) = input.as_object() {
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        req = req.query(&[(k.as_str(), s)]);
                    }
                }
            }
            req
        }
        XrpcMethod::Procedure => client.post(&url).json(&input),
    };

    let req_builder = req_builder
        .header("Authorization", format!("Bearer {}", pds.app_password))
        .header("atproto-proxy", &proxy_header);

    match req_builder.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.json::<Value>().await.unwrap_or_default();
            XrpcResponse { status, body }
        }
        Err(e) => {
            tracing::warn!(%url, %e, "PDS proxy request failed");
            XrpcResponse::error(502, "ErrPdsProxyFailed")
        }
    }
}

// ---------------------------------------------------------------------------
// Single XRPC handler
// ---------------------------------------------------------------------------

/// Single handler for all XRPC requests.
///
/// 1. Reads `atproto-proxy` header to identify the target arbiter DID.
/// 2. Extracts the NSID from the URL path.
/// 3. If the NSID is `createArbiter` and the arbiter doesn't exist yet,
///    bootstraps a new PDS-backed state machine.
/// 4. Otherwise routes the event through the existing state machine.
/// 5. For `deleteArbiter`, removes the arbiter from the collection after
///    a successful response.
#[handler]
pub async fn handle_xrpc(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);

    // ── Extract routing info ────────────────────────────────────────

    let did = match arbiter_did_from_proxy(req) {
        Some(d) => d,
        None => {
            res.status_code(salvo::http::StatusCode::BAD_REQUEST);
            res.render(Json(err(
                "missing atproto-proxy header (format: <did>#arbiter)",
            )));
            return;
        }
    };

    let nsid = match nsid_from_path(req) {
        Some(n) => n,
        None => {
            res.status_code(salvo::http::StatusCode::BAD_REQUEST);
            res.render(Json(err("invalid XRPC path")));
            return;
        }
    };

    let body = parse_body_or_query(req).await;
    let params = build_params(&body);

    // ── createArbiter (bootstrap with PDS account) ──────────────────

    if nsid == NSID::CREATE_ARBITER {
        let mut coll = state.arbiters.lock().await;
        if coll.arbiters.contains_key(&did) {
            res.render(Json(err("ErrArbiterAlreadyExists")));
            return;
        }

        let config = body.get("config").cloned().unwrap_or_default();
        let policy = config
            .get("policy")
            .and_then(|v| v.as_str())
            .unwrap_or(state.default_policy)
            .to_string();

        // Extract PDS account info from request body
        let pds_endpoint = match body
            .get("pdsEndpoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            Some(e) => e,
            None => {
                res.render(Json(err("ErrMissingParam: pdsEndpoint required")));
                return;
            }
        };

        let account_did: Did = match body
            .get("accountDid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            Some(d) => d,
            None => {
                res.render(Json(err("ErrMissingParam: accountDid required")));
                return;
            }
        };

        let app_password = match body
            .get("appPassword")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            Some(p) => p,
            None => {
                res.render(Json(err("ErrMissingParam: appPassword required")));
                return;
            }
        };

        let pds_account = PdsAccount {
            pds_endpoint,
            account_did,
            app_password,
        };

        coll.create_arbiter_with_pds(did, config, policy, caller, pds_account);
        res.render(Json(serde_json::json!({})));
        return;
    }

    // ── Route to state machine ──────────────────────────────────────

    let is_delete_arbiter = nsid == NSID::DELETE_ARBITER;
    let method = nsid_method(&nsid);

    let event = Event::IncomingXrpc {
        nsid,
        method,
        params,
        caller_did: caller,
    };

    let mut coll = state.arbiters.lock().await;
    if !coll.arbiters.contains_key(&did) {
        res.render(Json(err("ErrArbiterNotExists")));
        return;
    }

    let (status, body) = process_request(
        &mut coll,
        &state.client,
        &did,
        event,
    )
    .await;

    // If deleteArbiter succeeded, remove from collection.
    if is_delete_arbiter && status == 200 {
        coll.remove(&did);
    }

    res.status_code(
        salvo::http::StatusCode::from_u16(status)
            .unwrap_or(salvo::http::StatusCode::INTERNAL_SERVER_ERROR),
    );
    res.render(Json(body));
}