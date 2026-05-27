//! XRPC endpoint handlers for the `town.muni.arbiter.*` lexicons.
//!
//! Each handler:
//! 1. Extracts the caller DID from auth middleware
//! 2. Parses the request body / query params
//! 3. Processes the operation on the sans-IO core
//! 4. Handles any suspension loop (remote resolution)
//! 5. Returns the XRPC response

use std::sync::Arc;

use atproto_plc::{DidBuilder, ServiceEndpoint, SigningKey};
use salvo::prelude::*;
use salvo::writing::Json;

use arbiter_core::{NSID, OpResult, OpStep};

use crate::ServerState;

// ---------------------------------------------------------------------------
// Helper: extract caller DID from auth
// ---------------------------------------------------------------------------

fn caller_did(depot: &Depot) -> String {
    depot
        .get::<String>("caller_did")
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Helper: run an operation to completion, handling suspension loop
// ---------------------------------------------------------------------------

/// Run an operation on the core, resolving any remote suspensions by
/// fetching data from the remote arbiter over HTTP.
async fn run_operation(
    state: &ServerState,
    arbiter_did: &str,
    caller_did: &str,
    nsid: &str,
    params: serde_json::Value,
) -> OpStep {
    let mut core = state.core.lock().await;
    let step = core.process_operation(arbiter_did, caller_did, nsid, params.clone());
    resolve_loop(&mut core, &state.client, step).await
}

/// Recursively resolve suspensions by fetching remote or local data.
async fn resolve_loop(
    core: &mut arbiter_core::ArbiterCore,
    client: &reqwest::Client,
    step: OpStep,
) -> OpStep {
    match step {
        OpStep::Suspended { job_id, request } => match request {
            arbiter_core::CoreRequest::Local { path, input } => {
                // The policy is requesting data from this arbiter.
                // Execute the query directly, bypassing the policy check
                // (the policy itself is the one requesting this data).
                tracing::debug!(%path, "Resolving local XRPC request");
                let resolved = core.execute_local_query(&path, &input);
                let value = match &resolved {
                    OpStep::Done(OpResult::Ok(ok)) => {
                        serde_json::to_value(ok).unwrap_or(serde_json::json!({}))
                    }
                    _ => {
                        tracing::warn!(%path, "Local query returned non-Ok result");
                        serde_json::json!([])
                    }
                };
                let resumed = core.resume_operation(job_id, value);
                Box::pin(resolve_loop(core, client, resumed)).await
            }
            arbiter_core::CoreRequest::Remote {
                remote_did,
                path,
                input,
                caller_did: _,
            } => {
                tracing::info!(%remote_did, %path, "Resolving remote XRPC");
                // Fetch from remote arbiter
                let url = format!("/xrpc/{path}");
                let resolved = crate::io::resolve_remote(client, &remote_did, &url, &input).await;
                let resumed = core.resume_operation(job_id, resolved);
                Box::pin(resolve_loop(core, client, resumed)).await
            }
        },
        done => done,
    }
}

/// Helper: build XRPC response
fn error_response(error: &str) -> serde_json::Value {
    serde_json::json!({
        "error": error,
    })
}

/// Extract a String parameter from JSON body or query.
fn param_str(params: &serde_json::Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract arbiter DID from request (body for POST, query for GET).
async fn parse_body_or_query(req: &mut Request) -> serde_json::Value {
    if req.method() == salvo::http::Method::GET {
        // For GET requests, we extract individual query parameters manually.
        // Salvo provides query::<T>() for individual params.
        let mut map = serde_json::Map::new();
        if let Some(v) = req.query::<String>("arbiterDid") {
            map.insert("arbiterDid".into(), serde_json::Value::String(v));
        }
        if let Some(v) = req.query::<String>("spaceKey") {
            map.insert("spaceKey".into(), serde_json::Value::String(v));
        }
        serde_json::Value::Object(map)
    } else {
        req.parse_json::<serde_json::Value>()
            .await
            .unwrap_or_default()
    }
}

/// Extract arbiter DID from request (body for POST, query for GET).
fn get_arbiter_did(req: &Request, params: &serde_json::Value) -> Option<String> {
    // Try body/params first, then query string
    param_str(params, "arbiterDid").or_else(|| req.query::<String>("arbiterDid"))
}

fn get_space_key(params: &serde_json::Value) -> Option<String> {
    param_str(params, "spaceKey").or_else(|| param_str(params, "space_key"))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

// ---- createArbiter (procedure) ----

#[handler]
pub async fn create_arbiter(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let config = body.get("config").cloned().unwrap_or_default();

    let mut core = state.core.lock().await;
    let result = core.create_arbiter(arbiter_did, config, caller);

    match result {
        OpResult::Ok(_) => res.render(Json(serde_json::json!({}))),
        OpResult::Err(e) => res.render(Json(error_response(&e.error))),
    }
}

// ---- getArbiterConfig (query) ----

#[handler]
pub async fn get_arbiter_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": "$admin"});
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::GET_ARBITER_CONFIG,
        params,
    )
    .await;

    respond_step(step, res, |ok| serde_json::json!({ "config": ok.config }));
}

// ---- setArbiterConfig (procedure) ----

#[handler]
pub async fn set_arbiter_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let config = body.get("config").cloned().unwrap_or_default();
    let params = serde_json::json!({"spaceKey": "$admin", "config": config});
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::SET_ARBITER_CONFIG,
        params,
    )
    .await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- deleteArbiter (procedure) ----

#[handler]
pub async fn delete_arbiter(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": "$admin"});
    let step = run_operation(&state, &arbiter_did, &caller, NSID::DELETE_ARBITER, params).await;

    match step {
        OpStep::Deleted => res.render(Json(serde_json::json!({}))),
        OpStep::Done(OpResult::Ok(_)) => res.render(Json(serde_json::json!({}))),
        OpStep::Done(OpResult::Err(e)) => res.render(Json(error_response(&e.error))),
        OpStep::Suspended { .. } => res.render(Json(error_response("ErrTimeout"))),
        OpStep::ProxyRequest { .. } => res.render(Json(error_response("ErrUnexpectedProxy"))),
    }
}

// ---- createSpace (procedure) ----

#[handler]
pub async fn create_space(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let space_type = body
        .get("spaceType")
        .and_then(|v| v.as_str())
        .unwrap_or("town.muni.arbiter.config.space");
    let config = body.get("config").cloned().unwrap_or_default();

    let params = serde_json::json!({
        "spaceKey": space_key,
        "spaceType": space_type,
        "config": config,
    });
    let step = run_operation(&state, &arbiter_did, &caller, NSID::CREATE_SPACE, params).await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- getSpaceConfig (query) ----

#[handler]
pub async fn get_space_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": space_key});
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::GET_SPACE_CONFIG,
        params,
    )
    .await;

    respond_step(
        step,
        res,
        |ok| serde_json::json!({ "spaceType": "", "config": ok.config }),
    );
}

// ---- setSpaceConfig (procedure) ----

#[handler]
pub async fn set_space_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let space_type = body
        .get("spaceType")
        .and_then(|v| v.as_str())
        .unwrap_or("town.muni.arbiter.config.space");
    let config = body.get("config").cloned().unwrap_or_default();

    let params = serde_json::json!({
        "spaceKey": space_key,
        "spaceType": space_type,
        "config": config,
    });
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::SET_SPACE_CONFIG,
        params,
    )
    .await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- deleteSpace (procedure) ----

#[handler]
pub async fn delete_space(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": space_key});
    let step = run_operation(&state, &arbiter_did, &caller, NSID::DELETE_SPACE, params).await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- listSpaces (query) ----

#[handler]
pub async fn list_spaces(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": "$admin"});
    let step = run_operation(&state, &arbiter_did, &caller, NSID::LIST_SPACES, params).await;

    respond_step(step, res, |ok| serde_json::json!({ "spaces": ok.spaces }));
}

// ---- getSpaceMembers (query) ----

#[handler]
pub async fn get_space_members(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": space_key});
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::GET_SPACE_MEMBERS,
        params,
    )
    .await;

    respond_step(step, res, |ok| serde_json::json!({ "members": ok.members }));
}

// ---- resolveSpaceMembers (query) ----

#[handler]
pub async fn resolve_space_members(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let params = serde_json::json!({"spaceKey": space_key});
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::RESOLVE_SPACE_MEMBERS,
        params,
    )
    .await;

    respond_step(step, res, |ok| {
        serde_json::json!({
            "members": ok.members,
            "missingSpaces": ok.missing_spaces,
        })
    });
}

// ---- setSpaceMemberAccess (procedure) ----

#[handler]
pub async fn set_space_member_access(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let member = body
        .get("member")
        .and_then(|m| m.get("did"))
        .or_else(|| body.get("memberDid"));
    let member_did = match member.and_then(|v| v.as_str()) {
        Some(d) => d.to_string(),
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing member.did",
            )));
            return;
        }
    };
    let access = body.get("access").cloned().unwrap_or_default();

    let params = serde_json::json!({
        "spaceKey": space_key,
        "memberDid": member_did,
        "access": access,
    });
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::SET_SPACE_MEMBER_ACCESS,
        params,
    )
    .await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- removeSpaceMember (procedure) ----

#[handler]
pub async fn remove_space_member(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match get_arbiter_did(req, &body) {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing arbiterDid",
            )));
            return;
        }
    };
    let space_key = match get_space_key(&body) {
        Some(k) => k,
        None => {
            res.render(Json(error_response("ErrInvalidRequest: missing spaceKey")));
            return;
        }
    };

    let member_did = match body
        .get("member")
        .and_then(|m| m.get("did"))
        .or_else(|| body.get("memberDid"))
        .and_then(|v| v.as_str())
    {
        Some(d) => d.to_string(),
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing member.did",
            )));
            return;
        }
    };

    let params = serde_json::json!({
        "spaceKey": space_key,
        "memberDid": member_did,
    });
    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::REMOVE_SPACE_MEMBER,
        params,
    )
    .await;

    respond_step(step, res, |_| serde_json::json!({}));
}

// ---- createDid (procedure) ----

/// Create a new DID managed by this service.
///
/// This is a bootstrap operation — there's no existing arbiter to check
/// policy against. The caller becomes the owner of the new arbiter.
///
/// The DID is created as a proper did:plc identity using the atproto-plc
/// crate. A genesis operation is generated, signed, and submitted to the
/// configured PLC directory server.
#[handler]
pub async fn create_did(_req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);

    if caller.is_empty() {
        res.render(Json(error_response(
            "ErrInvalidRequest: missing caller DID (auth required)",
        )));
        return;
    }

    // Generate keys for the DID
    let rotation_key = SigningKey::generate_p256();
    let signing_key = SigningKey::generate_k256();

    let server_did = &state.server_did;
    let service_endpoint = format!(
        "https://{}/xrpc",
        server_did
            .strip_prefix("did:web:")
            .unwrap_or("localhost:8080")
            .replace("%3A", ":")
    );

    // Build the DID — this generates keys, creates and signs the genesis
    // operation, and derives the did:plc identifier from the hash.
    let (did, operation, keys) = match DidBuilder::new()
        .add_rotation_key(rotation_key)
        .add_verification_method("atproto".into(), signing_key)
        .add_service(
            "atproto_pds".into(),
            ServiceEndpoint::new(
                "AtprotoPersonalDataServer".into(),
                service_endpoint,
            ),
        )
        .build()
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(%e, "Failed to build DID");
            res.render(Json(error_response(&format!("ErrDidCreationFailed: {e}"))));
            return;
        }
    };

    let did_str = did.as_str().to_string();

    // Compute the genesis operation CID so we can sign future updates
    let genesis_cid = match operation.cid() {
        Ok(cid) => Some(cid),
        Err(e) => {
            tracing::warn!(%e, "Failed to compute genesis operation CID");
            None
        }
    };

    // Submit the genesis operation to the PLC directory
    if let Err(e) = crate::plc::submit_operation(&state, &did_str, &operation).await {
        tracing::error!(%did_str, %e, "Failed to submit genesis operation to PLC directory");
        res.render(Json(error_response(&format!("ErrPlcDirectoryUnreachable: {e}"))));
        return;
    }

    // Store the keys so we can sign future updates
    {
        let mut key_store = state.did_keys.lock().await;
        key_store.insert(
            did_str.clone(),
            crate::plc::DidState {
                keys: std::sync::Arc::new(keys),
                latest_cid: genesis_cid,
            },
        );
    }

    // Create the arbiter for this DID, with the caller as owner
    let mut core = state.core.lock().await;
    let result = core.create_arbiter(did_str.clone(), serde_json::json!({}), caller);
    drop(core);

    match result {
        OpResult::Ok(_) => {
            tracing::info!(%did_str, "Created new DID and arbiter");
            res.render(Json(serde_json::json!({"did": did_str})));
        }
        OpResult::Err(e) => {
            res.render(Json(error_response(&e.error)));
        }
    }
}

// ---- updateDidDoc (procedure) ----

/// Update the DID document for a DID hosted by this service.
///
/// Goes through the arbiter's policy. Only callers with Owner-level
/// access on the arbiter's $admin space can update the DID document.
///
/// On success, the update is submitted to the PLC directory as a signed
/// PLC operation.
#[handler]
pub async fn update_did_doc(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: serde_json::Value = req.parse_json().await.unwrap_or_default();

    let arbiter_did = match body
        .get("did")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        Some(d) => d,
        None => {
            res.render(Json(error_response(
                "ErrInvalidRequest: missing did",
            )));
            return;
        }
    };

    let config = body.get("config").cloned().unwrap_or_default();

    // Step 1: Run through the policy system — the arbiter must exist and
    // the caller must have Owner-level access.
    let params = serde_json::json!({
        "config": config,
    });

    let step = run_operation(
        &state,
        &arbiter_did,
        &caller,
        NSID::UPDATE_DID_DOC,
        params,
    )
    .await;

    // Step 2: If policy check failed, return the error
    match step {
        OpStep::Done(OpResult::Ok(_)) => {
            // Policy passed — proceed with PLC update
        }
        OpStep::Done(OpResult::Err(e)) => {
            res.render(Json(error_response(&e.error)));
            return;
        }
        OpStep::Suspended { .. } => {
            res.status_code(salvo::http::StatusCode::GATEWAY_TIMEOUT);
            res.render(Json(error_response("ErrTimeout")));
            return;
        }
        _ => {
            res.render(Json(error_response("ErrUnexpected")));
            return;
        }
    };

    // Step 3: Fetch current PLC state and latest operation CID
    let (plc_state, prev_cid) = match crate::plc::fetch_state_with_cid(&state, &arbiter_did).await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(%arbiter_did, %e, "Failed to fetch PLC state");
            res.render(Json(error_response(&format!(
                "ErrPlcDirectoryUnreachable: {e}"
            ))));
            return;
        }
    };

    // Step 4: Get the stored signing keys
    let did_state = {
        let key_store = state.did_keys.lock().await;
        key_store.get(&arbiter_did).cloned()
    };

    let signing_key = match did_state {
        Some(ref ds) => ds.keys.primary_rotation_key().cloned(),
        None => None,
    };

    let rotation_key = match signing_key {
        Some(k) => k,
        None => {
            res.render(Json(error_response(
                "ErrKeyNotFound: no signing keys stored for this DID",
            )));
            return;
        }
    };

    // Step 5: Merge the provided config with the current PLC state
    let rotation_keys = config
        .get("rotationKeys")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| plc_state.rotation_keys.clone());

    let verification_methods = config
        .get("verificationMethods")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_else(|| plc_state.verification_methods.clone());

    let also_known_as = config
        .get("alsoKnownAs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| plc_state.also_known_as.clone());

    let services = config
        .get("services")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| {
                    let endpoint = v.get("endpoint")?.as_str()?;
                    let service_type = v.get("type")?.as_str()?;
                    Some((
                        k.clone(),
                        atproto_plc::ServiceEndpoint::new(
                            service_type.to_string(),
                            endpoint.to_string(),
                        ),
                    ))
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_else(|| plc_state.services.clone());

    // Step 6: Build and sign the update operation
    let unsigned = atproto_plc::Operation::new_update(
        rotation_keys,
        verification_methods,
        also_known_as,
        services,
        prev_cid.clone(),
    );

    let signed = match unsigned.sign(&rotation_key) {
        Ok(op) => op,
        Err(e) => {
            tracing::error!(%arbiter_did, %e, "Failed to sign update operation");
            res.render(Json(error_response(&format!(
                "ErrSigningFailed: {e}"
            ))));
            return;
        }
    };

    // Step 7: Submit the update to the PLC directory
    if let Err(e) = crate::plc::submit_operation(&state, &arbiter_did, &signed).await {
        tracing::error!(%arbiter_did, %e, "Failed to submit update to PLC directory");
        res.render(Json(error_response(&format!(
            "ErrPlcDirectoryUnreachable: {e}"
        ))));
        return;
    }

    // Step 8: Update the stored CID
    if let Ok(new_cid) = signed.cid() {
        let mut key_store = state.did_keys.lock().await;
        if let Some(s) = key_store.get_mut(&arbiter_did) {
            s.latest_cid = Some(new_cid);
        }
    }

    tracing::info!(%arbiter_did, "DID document updated");
    res.render(Json(serde_json::json!({"did": arbiter_did})));
}

// ---------------------------------------------------------------------------
// Response helper
// ---------------------------------------------------------------------------

/// Map an `OpStep` to an HTTP response using the provided `map_ok` closure
/// to build the success body.
fn respond_step(
    step: OpStep,
    res: &mut Response,
    map_ok: impl FnOnce(arbiter_core::OpOk) -> serde_json::Value,
) {
    match step {
        OpStep::Done(OpResult::Ok(ok)) => {
            res.render(Json(map_ok(ok)));
        }
        OpStep::Done(OpResult::Err(e)) => {
            res.status_code(salvo::http::StatusCode::FORBIDDEN);
            res.render(Json(error_response(&e.error)));
        }
        OpStep::Deleted => {
            res.render(Json(serde_json::json!({})));
        }
        OpStep::Suspended { .. } => {
            res.status_code(salvo::http::StatusCode::GATEWAY_TIMEOUT);
            res.render(Json(error_response("ErrTimeout")));
        }
        OpStep::ProxyRequest { .. } => {
            // Should not reach here — proxy requests are handled
            // by `proxy_xrpc` and don't call `respond_step`.
            res.status_code(salvo::http::StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(error_response("ErrUnexpectedProxyResponse")));
        }
    }
}

// ---------------------------------------------------------------------------
// Proxy handler — for foreign (non-arbiter) XRPC methods
// ---------------------------------------------------------------------------

/// Handle a foreign XRPC method by checking the arbiter policy and
/// proxying to the configured backend if allowed.
#[handler]
pub async fn proxy_xrpc(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);

    // Extract NSID from the request path
    let path = req.uri().path();
    let nsid = path.strip_prefix("/xrpc/").unwrap_or(path).to_string();

    // Extract arbiter DID and params from body (POST) or query (GET)
    let params = if req.method() == salvo::http::Method::GET {
        let mut map = serde_json::Map::new();
        if let Some(v) = req.query::<String>("arbiterDid") {
            map.insert("arbiterDid".into(), serde_json::Value::String(v));
        }
        serde_json::Value::Object(map)
    } else {
        req.parse_json::<serde_json::Value>()
            .await
            .unwrap_or_default()
    };

    let arbiter_did = params
        .get("arbiterDid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| req.query::<String>("arbiterDid"))
        .unwrap_or_default();

    if arbiter_did.is_empty() {
        res.status_code(salvo::http::StatusCode::BAD_REQUEST);
        res.render(Json(error_response(
            "ErrInvalidRequest: missing arbiterDid",
        )));
        return;
    }

    // Run the policy check
    let step = run_operation(&state, &arbiter_did, &caller, &nsid, params).await;

    match step {
        OpStep::ProxyRequest {
            arbiter_did,
            caller_did: _,
            nsid,
            params,
        } => {
            // Read the backend URL from the arbiter's config
            let backend_url = {
                let core = state.core.lock().await;
                core.arbiters
                    .get(&arbiter_did)
                    .and_then(|a| a.config.get("backendUrl"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };

            let Some(backend_url) = backend_url else {
                res.status_code(salvo::http::StatusCode::BAD_GATEWAY);
                res.render(Json(error_response("ErrBackendNotConfigured")));
                return;
            };

            // Build the backend URL
            let proxy_url = format!("{}/xrpc/{}", backend_url.trim_end_matches('/'), nsid);

            // Proxy the request
            let client = &state.client;
            let proxy_req = if req.method() == salvo::http::Method::GET {
                client.get(&proxy_url)
            } else {
                // Forward the body as JSON
                client.post(&proxy_url).json(&params)
            };

            match proxy_req.send().await {
                Ok(backend_resp) => {
                    // Forward the status
                    let status = backend_resp.status();
                    res.status_code(
                        salvo::http::StatusCode::from_u16(status.as_u16())
                            .unwrap_or(salvo::http::StatusCode::OK),
                    );

                    // Forward the body — get bytes then try JSON
                    match backend_resp.bytes().await {
                        Ok(bytes) => {
                            if let Ok(body) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                                res.render(Json(body));
                            } else {
                                // Return as raw text
                                let text = String::from_utf8_lossy(&bytes);
                                res.render(text.to_string());
                            }
                        }
                        Err(_) => {
                            res.render(Json(serde_json::json!({})));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(%backend_url, %e, "Backend proxy failed");
                    res.status_code(salvo::http::StatusCode::BAD_GATEWAY);
                    res.render(Json(error_response("ErrBackendUnreachable")));
                }
            }
        }
        OpStep::Done(OpResult::Ok(_)) => {
            // Shouldn't happen for foreign methods, but handle gracefully
            res.render(Json(serde_json::json!({})));
        }
        OpStep::Done(OpResult::Err(e)) => {
            res.status_code(salvo::http::StatusCode::FORBIDDEN);
            res.render(Json(error_response(&e.error)));
        }
        OpStep::Deleted => {
            res.render(Json(serde_json::json!({})));
        }
        OpStep::Suspended { .. } => {
            res.status_code(salvo::http::StatusCode::GATEWAY_TIMEOUT);
            res.render(Json(error_response("ErrTimeout")));
        }
    }
}
