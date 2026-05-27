//! XRPC endpoint handlers for `town.muni.arbiter.*` lexicons.
//!
//! Each handler:
//! 1. Parses request (auth + body/query)
//! 2. Locks the arbiter collection
//! 3. Evaluates `data.arbiter.allow` via [`check_allow`] (handles suspensions)
//! 4. If denied → 403
//! 5. If allowed → manipulate state / return data directly

use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde_json::Value;

use crate::policy::{self, NSID};
use crate::state::{ArbiterCollection, MemberEntry, Space};
use crate::ServerState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn caller_did(depot: &Depot) -> String {
    depot.get::<String>("caller_did").cloned().unwrap_or_default()
}

fn err(msg: &str) -> Value {
    serde_json::json!({ "error": msg })
}

fn arbiter_did(body: &Value) -> Option<String> {
    body.get("arbiterDid").or_else(|| body.get("did"))
        .and_then(|v| v.as_str()).map(String::from)
}

fn space_key(body: &Value) -> Option<String> {
    body.get("spaceKey").or_else(|| body.get("space_key"))
        .and_then(|v| v.as_str()).map(String::from)
}

fn member_did(body: &Value) -> Option<String> {
    body.get("memberDid")
        .or_else(|| body.get("member").and_then(|m| m.get("did")))
        .and_then(|v| v.as_str()).map(String::from)
}

async fn parse_body_or_query(req: &mut Request) -> Value {
    if req.method() == salvo::http::Method::GET {
        let mut map = serde_json::Map::new();
        if let Some(v) = req.query::<String>("arbiterDid") { map.insert("arbiterDid".into(), Value::String(v)); }
        if let Some(v) = req.query::<String>("spaceKey") { map.insert("spaceKey".into(), Value::String(v)); }
        Value::Object(map)
    } else {
        req.parse_json::<Value>().await.unwrap_or_default()
    }
}

/// Evaluate `data.arbiter.allow` for an operation. Borrows `coll` immutably
/// during the suspension loop, then releases the borrow on return.
async fn check_allow(
    coll: &ArbiterCollection,
    client: &reqwest::Client,
    arbiter_did: &str,
    caller_did: &str,
    nsid: &str,
    params: &Value,
) -> Result<bool, String> {
    let arbiter = coll.get(arbiter_did)
        .ok_or_else(|| "ErrArbiterNotExists".to_string())?;
    let result = policy::evaluate(
        &arbiter.policy, &arbiter.config,
        caller_did, nsid, params,
        &["data.arbiter.allow"],
        arbiter_did, coll, client,
    ).await?;
    Ok(result == Value::Bool(true))
}

fn deny(res: &mut Response) {
    res.status_code(salvo::http::StatusCode::FORBIDDEN);
    res.render(Json(err("ErrPermissionDenied")));
}

// ---------------------------------------------------------------------------
// createArbiter (bootstrap — no policy)
// ---------------------------------------------------------------------------

#[handler]
pub async fn create_arbiter(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();

    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let config = body.get("config").cloned().unwrap_or_default();
    let policy = config.get("policy").and_then(|v| v.as_str()).unwrap_or(state.default_policy).to_string();

    let mut coll = state.arbiters.lock().await;
    if coll.arbiters.contains_key(&did) { res.render(Json(err("ErrArbiterAlreadyExists"))); return; }
    coll.create_arbiter(did, config, policy, caller);
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// getArbiterConfig (query)
// ---------------------------------------------------------------------------

#[handler]
pub async fn get_arbiter_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};

    let coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::GET_ARBITER_CONFIG, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let config = coll.get(&did).map(|a| &a.config).cloned();
    res.render(Json(serde_json::json!({ "config": config })));
}

// ---------------------------------------------------------------------------
// setArbiterConfig (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn set_arbiter_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};

    let mut coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::SET_ARBITER_CONFIG, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let new_config = body.get("config").cloned().unwrap_or_default();
    if let Some(arb) = coll.arbiters.get_mut(&did) {
        arb.config = new_config;
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// deleteArbiter (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn delete_arbiter(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};

    let mut coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::DELETE_ARBITER, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    coll.arbiters.remove(&did);
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// createSpace (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn create_space(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();

    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    let mut coll = state.arbiters.lock().await;
    // Pre-check: arbiter and space existence
    if coll.get(&did).is_none() { res.render(Json(err("ErrArbiterNotExists"))); return; }
    if coll.arbiters.get(&did).unwrap().spaces.contains_key(&sk) { res.render(Json(err("ErrSpaceExists"))); return; }

    let params = serde_json::json!({"spaceKey": sk, "spaceType": body.get("spaceType"), "config": body.get("config")});
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::CREATE_SPACE, &params).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let space_type = body.get("spaceType").and_then(|v| v.as_str()).unwrap_or("town.muni.arbiter.config.space").to_string();
    let config = body.get("config").cloned().unwrap_or_default();
    if let Some(arb) = coll.arbiters.get_mut(&did) {
        arb.spaces.insert(sk.clone(), Space { key: sk, space_type, config, members: vec![] });
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// getSpaceConfig (query)
// ---------------------------------------------------------------------------

#[handler]
pub async fn get_space_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    let coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::GET_SPACE_CONFIG, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let config = coll.space_config(&did, &sk);
    res.render(Json(serde_json::json!({ "config": config, "spaceType": "" })));
}

// ---------------------------------------------------------------------------
// setSpaceConfig (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn set_space_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    let mut coll = state.arbiters.lock().await;
    if coll.get(&did).map(|a| a.spaces.contains_key(&sk)).unwrap_or(false) == false {
        res.render(Json(err("ErrSpaceNotExists"))); return;
    }

    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::SET_SPACE_CONFIG, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let space_type = body.get("spaceType").and_then(|v| v.as_str()).unwrap_or("town.muni.arbiter.config.space").to_string();
    let config = body.get("config").cloned().unwrap_or_default();
    if let Some(arb) = coll.arbiters.get_mut(&did) {
        if let Some(space) = arb.spaces.get_mut(&sk) {
            space.space_type = space_type;
            space.config = config;
        }
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// deleteSpace (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn delete_space(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    if sk == "$admin" { res.render(Json(err("ErrCannotDeleteAdminSpace"))); return; }

    let mut coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::DELETE_SPACE, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    if let Some(arb) = coll.arbiters.get_mut(&did) {
        arb.spaces.remove(&sk);
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// listSpaces (query)
// ---------------------------------------------------------------------------

#[handler]
pub async fn list_spaces(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};

    let coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::LIST_SPACES, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let spaces: Vec<Value> = coll.get(&did).map(|a| a.spaces.values().map(|s| serde_json::json!({
        "spaceKey": s.key, "spaceType": s.space_type, "config": s.config,
    })).collect()).unwrap_or_default();
    res.render(Json(serde_json::json!({ "spaces": spaces })));
}

// ---------------------------------------------------------------------------
// getSpaceMembers (query)
// ---------------------------------------------------------------------------

#[handler]
pub async fn get_space_members(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    let coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::GET_SPACE_MEMBERS, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let members: Vec<Value> = coll.get(&did).and_then(|a| a.spaces.get(&sk))
        .map(|s| s.members.iter().map(|m| serde_json::json!({
            "member": { "did": m.did }, "access": m.access,
        })).collect()).unwrap_or_default();
    res.render(Json(serde_json::json!({ "members": members })));
}

// ---------------------------------------------------------------------------
// resolveSpaceMembers (query)
// ---------------------------------------------------------------------------

#[handler]
pub async fn resolve_space_members(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body = parse_body_or_query(req).await;
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};

    let coll = state.arbiters.lock().await;
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::RESOLVE_SPACE_MEMBERS, &body).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let result = match evaluate_resolve_result(&coll, &state.client, &did, &caller, &sk).await {
        Ok(v) => v,
        Err(e) => { res.render(Json(err(&e))); return; }
    };
    let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
    let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
    res.render(Json(serde_json::json!({ "members": members, "missingSpaces": missing })));
}

// ---------------------------------------------------------------------------
// setSpaceMemberAccess (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn set_space_member_access(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};
    let md = match member_did(&body) { Some(m) => m, None => { res.render(Json(err("missing member.did"))); return; }};

    let mut coll = state.arbiters.lock().await;
    let params = serde_json::json!({"spaceKey": sk, "memberDid": md, "access": body.get("access")});
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::SET_SPACE_MEMBER_ACCESS, &params).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    let access = body.get("access").cloned().unwrap_or_default();
    if let Some(arb) = coll.arbiters.get_mut(&did) {
        if let Some(space) = arb.spaces.get_mut(&sk) {
            if let Some(existing) = space.members.iter_mut().find(|m| m.did == md) {
                existing.access = access;
            } else {
                space.members.push(MemberEntry { did: md, access });
            }
        }
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// removeSpaceMember (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn remove_space_member(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match arbiter_did(&body) { Some(d) => d, None => { res.render(Json(err("missing arbiterDid"))); return; }};
    let sk = match space_key(&body) { Some(k) => k, None => { res.render(Json(err("missing spaceKey"))); return; }};
    let md = match member_did(&body) { Some(m) => m, None => { res.render(Json(err("missing member.did"))); return; }};

    let mut coll = state.arbiters.lock().await;
    let params = serde_json::json!({"spaceKey": sk, "memberDid": md});
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::REMOVE_SPACE_MEMBER, &params).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    if let Some(arb) = coll.arbiters.get_mut(&did) {
        if let Some(space) = arb.spaces.get_mut(&sk) {
            space.members.retain(|m| m.did != md);
        }
        arb.version += 1;
    }
    res.render(Json(serde_json::json!({})));
}

// ---------------------------------------------------------------------------
// createDid (bootstrap — no policy)
// ---------------------------------------------------------------------------

#[handler]
pub async fn create_did(_req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    if caller.is_empty() { res.render(Json(err("auth required"))); return; }

    // Generate keys and DID via atproto-plc
    let rotation_key = atproto_plc::SigningKey::generate_p256();
    let signing_key = atproto_plc::SigningKey::generate_k256();
    let service_endpoint = format!("https://{}/xrpc", state.server_did.strip_prefix("did:web:").unwrap_or("localhost:8080").replace("%3A", ":"));

    let (did, operation, keys) = match atproto_plc::DidBuilder::new()
        .add_rotation_key(rotation_key)
        .add_verification_method("atproto".into(), signing_key)
        .add_service("atproto_pds".into(), atproto_plc::ServiceEndpoint::new("AtprotoPersonalDataServer".into(), service_endpoint))
        .build()
    {
        Ok(r) => r,
        Err(e) => { res.render(Json(err(&format!("ErrDidCreation: {e}")))); return; }
    };

    let did_str = did.as_str().to_string();
    let genesis_cid = operation.cid().ok();

    // Submit to PLC directory
    if let Err(e) = crate::plc::submit_operation(&state, &did_str, &operation).await {
        res.render(Json(err(&format!("ErrPlcDirectory: {e}")))); return;
    }

    // Store keys
    {
        let mut ks = state.did_keys.lock().await;
        ks.insert(did_str.clone(), crate::plc::DidState { keys: std::sync::Arc::new(keys), latest_cid: genesis_cid });
    }

    // Create arbiter for the DID
    let mut coll = state.arbiters.lock().await;
    coll.create_arbiter(did_str.clone(), serde_json::json!({}), state.default_policy.to_string(), caller);
    res.render(Json(serde_json::json!({ "did": did_str })));
}

// ---------------------------------------------------------------------------
// updateDidDoc (procedure)
// ---------------------------------------------------------------------------

#[handler]
pub async fn update_did_doc(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let body: Value = req.parse_json().await.unwrap_or_default();
    let did = match body.get("did").and_then(|v| v.as_str()).map(String::from) {
        Some(d) => d, None => { res.render(Json(err("missing did"))); return; }
    };
    let config = body.get("config").cloned().unwrap_or_default();

    let coll = state.arbiters.lock().await;
    let params = serde_json::json!({ "config": config });
    let allowed = match check_allow(&coll, &state.client, &did, &caller, NSID::UPDATE_DID_DOC, &params).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }
    drop(coll);

    // Fetch current PLC state and build update operation
    let (plc_state, prev_cid) = match crate::plc::fetch_state_with_cid(&state, &did).await {
        Ok(r) => r,
        Err(e) => { res.render(Json(err(&format!("ErrPlcDirectory: {e}")))); return; }
    };

    let did_state = { let ks = state.did_keys.lock().await; ks.get(&did).cloned() };
    let rotation_key = match did_state.and_then(|s| s.keys.primary_rotation_key().cloned()) {
        Some(k) => k,
        None => { res.render(Json(err("ErrKeyNotFound"))); return; }
    };

    // Merge config with current state
    let merge = |field: &str, default: Value| -> Value {
        config.get(field).cloned().unwrap_or(default)
    };

    let rotation_keys = merge("rotationKeys", serde_json::json!(plc_state.rotation_keys))
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
    let verification_methods = merge("verificationMethods", serde_json::json!(plc_state.verification_methods))
        .as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect::<std::collections::HashMap<_, _>>()).unwrap_or_default();
    let also_known_as = merge("alsoKnownAs", serde_json::json!(plc_state.also_known_as))
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
    let services = merge("services", serde_json::json!(plc_state.services))
        .as_object().map(|o| o.iter().filter_map(|(k, v)| {
            Some((k.clone(), atproto_plc::ServiceEndpoint::new(v.get("type")?.as_str()?.to_string(), v.get("endpoint")?.as_str()?.to_string())))
        }).collect::<std::collections::HashMap<_, _>>()).unwrap_or_default();

    let unsigned = atproto_plc::Operation::new_update(rotation_keys, verification_methods, also_known_as, services, prev_cid);
    let signed = match unsigned.sign(&rotation_key) {
        Ok(op) => op, Err(e) => { res.render(Json(err(&format!("ErrSigning: {e}")))); return; }
    };

    // Submit to PLC directory
    if let Err(e) = crate::plc::submit_operation(&state, &did, &signed).await {
        res.render(Json(err(&format!("ErrPlcDirectory: {e}")))); return;
    }

    if let Ok(new_cid) = signed.cid() {
        let mut ks = state.did_keys.lock().await;
        if let Some(s) = ks.get_mut(&did) { s.latest_cid = Some(new_cid); }
    }

    res.render(Json(serde_json::json!({ "did": did })));
}

// ---------------------------------------------------------------------------
// proxy_xrpc — for foreign (non-arbiter) XRPC methods
// ---------------------------------------------------------------------------

#[handler]
pub async fn proxy_xrpc(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let state = depot.get::<Arc<ServerState>>("state").cloned().unwrap();
    let caller = caller_did(depot);
    let path = req.uri().path().strip_prefix("/xrpc/").unwrap_or("").to_string();
    let body = parse_body_or_query(req).await;

    let did = arbiter_did(&body).or_else(|| req.query::<String>("arbiterDid")).unwrap_or_default();
    if did.is_empty() { res.status_code(salvo::http::StatusCode::BAD_REQUEST); res.render(Json(err("missing arbiterDid"))); return; }

    let coll = state.arbiters.lock().await;
    let check_params = serde_json::json!({});
    let allowed = match check_allow(&coll, &state.client, &did, &caller, &path, &check_params).await {
        Ok(a) => a, Err(e) => { res.render(Json(err(&e))); return; }
    };
    if !allowed { deny(res); return; }

    // Get backend URL
    let backend_url = coll.get(&did).and_then(|a| a.config.get("backendUrl")).and_then(|v| v.as_str()).map(String::from);
    drop(coll);

    let Some(backend_url) = backend_url else {
        res.status_code(salvo::http::StatusCode::BAD_GATEWAY);
        res.render(Json(err("ErrBackendNotConfigured")));
        return;
    };

    let proxy_url = format!("{}/xrpc/{}", backend_url.trim_end_matches('/'), path);
    match state.client.get(&proxy_url).send().await {
        Ok(backend_resp) => {
            let status = backend_resp.status();
            res.status_code(salvo::http::StatusCode::from_u16(status.as_u16()).unwrap_or(salvo::http::StatusCode::OK));
            match backend_resp.bytes().await {
                Ok(bytes) => {
                    if let Ok(body) = serde_json::from_slice::<Value>(&bytes) { res.render(Json(body)); }
                    else { res.render(String::from_utf8_lossy(&bytes).to_string()); }
                }
                Err(_) => { res.render(Json(serde_json::json!({}))); }
            }
        }
        Err(e) => {
            tracing::error!(%backend_url, %e, "Backend proxy failed");
            res.status_code(salvo::http::StatusCode::BAD_GATEWAY);
            res.render(Json(err("ErrBackendUnreachable")));
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: evaluate_resolve_result
// ---------------------------------------------------------------------------

async fn evaluate_resolve_result(
    coll: &ArbiterCollection,
    client: &reqwest::Client,
    arbiter_did: &str,
    caller_did: &str,
    space_key: &str,
) -> Result<Value, String> {
    let arbiter = coll.get(arbiter_did).ok_or_else(|| "ErrArbiterNotExists".to_string())?;
    let params = serde_json::json!({ "spaceKey": space_key });
    policy::evaluate(
        &arbiter.policy, &arbiter.config,
        caller_did, NSID::RESOLVE_SPACE_MEMBERS, &params,
        &["data.arbiter.resolve_result"],
        arbiter_did, coll, client,
    ).await
}
