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

use std::sync::{Arc, LazyLock};

use arbiter_core::{Event, IoAction, NSID, XrpcResponse, nsid_method, policy_core::XrpcMethod};
use atproto_identity::validation::{is_valid_did_method_plc, is_valid_did_method_web};
use atrium_api::agent::{
    Configure,
    atp_agent::{CredentialSession, store::MemorySessionStore},
};
use atrium_xrpc::{InputDataOrBytes, OutputDataOrBytes, XrpcClient, XrpcRequest, http};
use atrium_xrpc_client::reqwest::ReqwestClient;
use moka::sync::Cache;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    ServerState,
    auth::CallerDid,
    resolver::RESOLVER,
    state::{ArbiterCollection, PdsCredentials},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAppPasswordArbiterInput {
    arbiter_did: String,
    app_password: String,
    config: Value,
}

/// Create a JSON error response from a message or error.
fn err<M: std::fmt::Display>(msg: M) -> Value {
    serde_json::json!({ "error": msg.to_string() })
}

/// Extract the target arbiter DID from the `atproto-proxy` header.
///
/// Format: `{did}#{serviceId}`  e.g. `did:plc:abc123#arbiter`
fn arbiter_did_from_req(req: &Request) -> anyhow::Result<&str> {
    let proxy_header: &str = req
        .header("atproto-proxy")
        .ok_or(anyhow::format_err!("Must provide atproto-proxy header"))?;
    let did = proxy_header
        .strip_suffix("#arbiter")
        .ok_or(anyhow::format_err!(
            "Invalid atproto-proxy header, requires `#arbiter` suffix."
        ))?;
    if !is_valid_did_method_web(did, true) && !is_valid_did_method_plc(did) {
        anyhow::bail!("Could not parse atproto-proxy DID");
    }
    Ok(did)
}

async fn xrpc_params_from_req(req: &mut Request) -> anyhow::Result<Value> {
    if req.method() == salvo::http::Method::GET {
        let mut map = serde_json::Map::new();
        for (key, values) in req.queries() {
            let Some(last_value) = values.last() else {
                continue;
            };
            map.insert(key.clone(), Value::String(last_value.clone()));
        }
        Ok(Value::Object(map))
    } else {
        Ok(req.parse_json::<Value>().await?)
    }
}

/// Feed an [`Event`] to an arbiter's [`StateMachine`] and drive any remote
/// requests to completion, returning the final response.
///
/// If the arbiter has an associated PDS account (app-password arbiter),
/// remote XRPC requests are proxied through that PDS instead of being
/// resolved directly.
async fn process_request(
    coll: &mut ArbiterCollection,
    arbiter_did: &str,
    event: Event,
) -> (u16, Value) {
    let mut stack = vec![event];

    while let Some(event) = stack.pop() {
        let actions = match coll.arbiters.get_mut(arbiter_did) {
            Some(sm) => sm.handle_event(event),
            None => {
                return (404, serde_json::json!({"error": "ErrArbiterNotExists"}));
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
                    // Proxy the outgoing requests through the PDS
                    let pds_account = coll
                        .get_pds_credential(arbiter_did)
                        .cloned()
                        .expect("every arbiter must have a PDS account");
                    let resp = resolve_remote_request(
                        arbiter_did,
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

    (500, serde_json::json!({"error": "ErrNoResponse"}))
}

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
    if let Err(e) = handle(req, depot, res).await {
        res.status_code(StatusCode::BAD_REQUEST)
            .render(Json(serde_json::json!({
                "error": e.to_string()
            })));
    }
}

// TODO: better error handling instead of "every error is 400"
async fn handle(req: &mut Request, depot: &mut Depot, res: &mut Response) -> anyhow::Result<()> {
    let state = depot.obtain::<ServerState>().expect("server state");
    let caller = depot.obtain::<CallerDid>().expect("caller did");
    let xrpc_params = xrpc_params_from_req(req).await?;
    let nsid = req.param::<&str>("nsid").expect("nsid in path");
    let arbiter_did = arbiter_did_from_req(req)?;

    // Handle arbiter creation
    if nsid == NSID::CREATE_APP_PASSWORD_ARBITER && req.method() == salvo::http::Method::POST {
        let mut coll = state.arbiters.lock().await;
        if coll.arbiters.contains_key(arbiter_did) {
            res.render(Json(err("ErrArbiterAlreadyExists")));
            return Ok(());
        }

        let input = serde_json::from_value::<CreateAppPasswordArbiterInput>(xrpc_params)?;

        let pds_account = PdsCredentials {
            app_password: input.app_password.to_string(),
        };

        coll.create_arbiter_with_app_password(
            input.arbiter_did.to_string(),
            serde_json::to_value(input.config).unwrap_or_default(),
            pds_account,
        )?;
        res.render(Json(serde_json::json!({})));
        return Ok(());
    }

    // ── Route to state machine ──────────────────────────────────────

    let is_delete_arbiter = nsid == NSID::DELETE_ARBITER;
    let method = nsid_method(nsid);

    let event = Event::IncomingXrpc {
        nsid: nsid.into(),
        method,
        params: xrpc_params,
        caller_did: caller.to_string(),
    };

    let mut coll = state.arbiters.lock().await;
    if !coll.arbiters.contains_key(arbiter_did) {
        res.render(Json(err("ErrArbiterNotExists")));
        return Ok(());
    }

    let (status, body) = process_request(&mut coll, arbiter_did, event).await;

    // If deleteArbiter succeeded, remove from collection.
    if is_delete_arbiter && status == 200 {
        coll.remove(arbiter_did);
    }

    res.status_code(
        salvo::http::StatusCode::from_u16(status)
            .unwrap_or(salvo::http::StatusCode::INTERNAL_SERVER_ERROR),
    );
    res.render(Json(body));

    Ok(())
}

type Session = Arc<CredentialSession<MemorySessionStore, ReqwestClient>>;
static SESSION_CACHE: LazyLock<Cache<String, Session>> =
    LazyLock::new(|| Cache::builder().max_capacity(16).build());

async fn get_session(did: &str, pds_url: &str, app_password: &str) -> anyhow::Result<Session> {
    let cached = SESSION_CACHE.get(did);
    if let Some(session) = cached {
        return Ok(session);
    }
    let client = ReqwestClient::new(pds_url);
    let session = Arc::new(CredentialSession::new(
        client,
        MemorySessionStore::default(),
    ));
    session.login(did, app_password).await?;

    SESSION_CACHE.insert(did.into(), session.clone());

    Ok(session)
}

/// Proxy a remote XRPC request through the arbiter's PDS account.
///
/// The PDS URL is used as the base endpoint, the app password is
/// included in the `Authorization` header, and the target DID+service
/// fragment is passed in the `atproto-proxy` header.
async fn resolve_remote_request(
    pds_did: &str,
    pds_creds: &PdsCredentials,
    target_did_and_service: &str,
    method: &XrpcMethod,
    nsid: &str,
    input: Value,
) -> XrpcResponse {
    let did_doc = RESOLVER.resolve(pds_did).await.unwrap();
    let pds_url = did_doc.pds_endpoints().into_iter().next().unwrap();
    let url = format!("{}/xrpc/{}", pds_url, nsid.trim_start_matches('/'));
    let Ok(session) = get_session(pds_did, pds_url, &pds_creds.app_password).await else {
        return XrpcResponse::error(
            500,
            "Could not open AppPassword session to the arbiter's PDS.",
        );
    };

    let req = XrpcRequest {
        method: match method {
            XrpcMethod::Query => http::Method::GET,
            XrpcMethod::Procedure => http::Method::POST,
        },
        nsid: nsid.into(),
        parameters: if let XrpcMethod::Query = method {
            Some(input.clone())
        } else {
            None
        },
        input: if let XrpcMethod::Procedure = method {
            Some(InputDataOrBytes::Data(input))
        } else {
            None
        },
        encoding: Some("application/json".into()),
    };

    let Some((target_did, target_service)) = target_did_and_service.split_once('#') else {
        return XrpcResponse::error(
            500,
            "Target did for routed XRPC requests must include a #service specification",
        );
    };
    let Ok(target_did) = target_did.parse() else {
        return XrpcResponse::error(500, "Could not parse target DID");
    };
    session.configure_proxy_header(target_did, target_service);
    match session.send_xrpc::<_, _, Value, Value>(&req).await {
        Ok(body) => {
            let OutputDataOrBytes::Data(body) = body else {
                return XrpcResponse::error(
                    500,
                    "Got bytes from XRPC which cannot be handled by Rego",
                );
            };
            XrpcResponse {
                // TODO: actually figure out the status?
                status: 200,
                body,
            }
        }
        Err(e) => {
            tracing::warn!(%url, %e, "PDS proxy request failed");
            XrpcResponse::error(502, "ErrPdsProxyFailed")
        }
    }
}
