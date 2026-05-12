//! XRPC endpoint handlers for the arbiter server.
//!
//! Each handler extracts the authenticated user, parses the request body or
//! query parameters, and delegates to `AsyncArbiterServer::handle_request`.

use std::sync::Arc;

use salvo::http::StatusError;
use salvo::{Depot, Request, Response};
use salvo::prelude::*;
use serde::Deserialize;

use arbiter_core::{
    Access, JobResult, Member, MessageKind, ServerError,
    futures::AsyncArbiterServer,
};

use crate::auth::get_authenticated_user;
use crate::io::HttpArbiterIo;

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

/// Convert a `ServerError` to a `StatusError` with a JSON error body.
fn convert_server_error(err: ServerError) -> StatusError {
    match err {
        ServerError::ArbiterAlreadyExists => {
            StatusError::bad_request()
                .detail("Arbiter already exists")
        }
        ServerError::ArbiterNotExists => {
            StatusError::not_found()
                .detail("Arbiter not found")
        }
        ServerError::ArbiterErr(inner) => {
            StatusError::bad_request()
                .detail(format!("Arbiter error: {inner:?}"))
        }
        ServerError::DuplicateReqId => {
            StatusError::internal_server_error()
                .detail("Duplicate request ID (internal error)")
        }
    }
}

// ---------------------------------------------------------------------------
// Helper to get the arbiter server from depot
// ---------------------------------------------------------------------------

fn get_server(
    depot: &Depot,
) -> Result<&Arc<AsyncArbiterServer<HttpArbiterIo>>, StatusError> {
    depot
        .get::<Arc<AsyncArbiterServer<HttpArbiterIo>>>("server")
        .map_err(|_| StatusError::internal_server_error().detail("Server not available"))
}

/// Respond with a JSON value.
fn json_response(res: &mut Response, value: serde_json::Value) {
    res.render(Json(value));
}

// ---------------------------------------------------------------------------
// Request/Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ArbiterActionInput {
    pub arbiter_did: String,
    #[serde(default = "default_resolver_depth")]
    pub resolver_depth: i64,
}

fn default_resolver_depth() -> i64 {
    3
}

#[derive(Debug, Deserialize)]
pub struct SpaceActionInput {
    pub arbiter_did: String,
    pub space_key: String,
    #[serde(default = "default_resolver_depth")]
    pub resolver_depth: i64,
}

#[derive(Debug, Deserialize)]
pub struct ConfigureSpaceInput {
    pub arbiter_did: String,
    pub space_key: String,
    pub public_records: bool,
    pub public_members: bool,
    #[serde(default = "default_resolver_depth")]
    pub resolver_depth: i64,
}

#[derive(Debug, Deserialize)]
pub struct SetMemberAccessInput {
    pub arbiter_did: String,
    pub space_key: String,
    pub member: Member,
    pub access: Access,
    #[serde(default = "default_resolver_depth")]
    pub resolver_depth: i64,
}

#[derive(Debug, Deserialize)]
pub struct RemoveMemberInput {
    pub arbiter_did: String,
    pub space_key: String,
    pub member: Member,
    #[serde(default = "default_resolver_depth")]
    pub resolver_depth: i64,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /xrpc/town.muni.arbiter.createArbiter
#[handler]
pub async fn create_arbiter(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: ArbiterActionInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            "$admin",
            body.resolver_depth,
            MessageKind::CreateArbiter,
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.deleteArbiter
#[handler]
pub async fn delete_arbiter(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: ArbiterActionInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            "$admin",
            body.resolver_depth,
            MessageKind::DeleteArbiter,
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// GET /xrpc/town.muni.arbiter.getMembers
#[handler]
pub async fn get_members(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;

    let arbiter_did = req
        .query::<String>("arbiterDid")
        .ok_or_else(|| StatusError::bad_request().detail("Missing arbiterDid"))?;
    let space_key = req
        .query::<String>("spaceKey")
        .ok_or_else(|| StatusError::bad_request().detail("Missing spaceKey"))?;
    let resolver_depth: i64 = req.query::<i64>("resolverDepth").unwrap_or(3);

    let result = server
        .handle_request(
            user_did,
            &arbiter_did,
            &space_key,
            resolver_depth,
            MessageKind::FetchMembers,
        )
        .await;

    match result {
        Ok(JobResult::ResolvedMembersList(list)) => {
            json_response(
                res,
                serde_json::json!({
                    "member_list": list.member_list,
                    "missing_spaces": list.missing_spaces,
                }),
            );
            Ok(())
        }
        Ok(JobResult::Ok) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// GET /xrpc/town.muni.arbiter.resolveMembers
///
/// Internal server-to-server endpoint.
#[handler]
pub async fn resolve_members(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;

    let space_key = req
        .query::<String>("spaceKey")
        .ok_or_else(|| StatusError::bad_request().detail("Missing spaceKey"))?;

    let result = server
        .handle_request(
            user_did,
            user_did,
            &space_key,
            0,
            MessageKind::FetchMembers,
        )
        .await;

    match result {
        Ok(JobResult::ResolvedMembersList(list)) => {
            json_response(res, serde_json::to_value(&list).unwrap_or_default());
            Ok(())
        }
        Ok(JobResult::Ok) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.createSpace
#[handler]
pub async fn create_space(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: SpaceActionInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            &body.space_key,
            body.resolver_depth,
            MessageKind::CreateSpace,
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.deleteSpace
#[handler]
pub async fn delete_space(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: SpaceActionInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            &body.space_key,
            body.resolver_depth,
            MessageKind::DeleteSpace,
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.configureSpace
#[handler]
pub async fn configure_space(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: ConfigureSpaceInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            &body.space_key,
            body.resolver_depth,
            MessageKind::ConfigureSpace {
                public_records: body.public_records,
                public_members: body.public_members,
            },
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.setMemberAccess
#[handler]
pub async fn set_member_access(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: SetMemberAccessInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            &body.space_key,
            body.resolver_depth,
            MessageKind::SetMemberAccess {
                member: body.member,
                access: body.access,
            },
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}

/// POST /xrpc/town.muni.arbiter.removeMember
#[handler]
pub async fn remove_member(
    depot: &mut Depot,
    req: &mut Request,
    res: &mut Response,
) -> Result<(), StatusError> {
    let user_did = get_authenticated_user(depot)?;
    let server = get_server(depot)?;
    let body: RemoveMemberInput = req
        .parse_json()
        .await
        .map_err(|e| StatusError::bad_request().detail(e.to_string()))?;

    let result = server
        .handle_request(
            user_did,
            &body.arbiter_did,
            &body.space_key,
            body.resolver_depth,
            MessageKind::RemoveMember {
                member: body.member,
            },
        )
        .await;

    match result {
        Ok(_) => {
            json_response(res, serde_json::json!({}));
            Ok(())
        }
        Err(e) => Err(convert_server_error(e)),
    }
}
