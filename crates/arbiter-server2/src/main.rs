//! Muni Town Arbiter Server v2 — AT Protocol XRPC HTTP server.
//!
//! Implements the `town.muni.arbiter.*` XRPC lexicons using Salvo,
//! with the sans-IO [`arbiter_core3`] state machine underneath.
//!
//! # Endpoints
//!
//! | Method | Path | Handler |
//! |--------|------|---------|
//! | POST | `/xrpc/town.muni.arbiter.createArbiter` | `create_arbiter` |
//! | POST | `/xrpc/town.muni.arbiter.setArbiterConfig` | `set_arbiter_config` |
//! | GET | `/xrpc/town.muni.arbiter.getArbiterConfig` | `get_arbiter_config` |
//! | POST | `/xrpc/town.muni.arbiter.deleteArbiter` | `delete_arbiter` |
//! | POST | `/xrpc/town.muni.arbiter.createSpace` | `create_space` |
//! | POST | `/xrpc/town.muni.arbiter.setSpaceConfig` | `set_space_config` |
//! | GET | `/xrpc/town.muni.arbiter.getSpaceConfig` | `get_space_config` |
//! | POST | `/xrpc/town.muni.arbiter.deleteSpace` | `delete_space` |
//! | GET | `/xrpc/town.muni.arbiter.listSpaces` | `list_spaces` |
//! | GET | `/xrpc/town.muni.arbiter.getSpaceMembers` | `get_space_members` |
//! | GET | `/xrpc/town.muni.arbiter.resolveSpaceMembers` | `resolve_space_members` |
//! | POST | `/xrpc/town.muni.arbiter.setSpaceMemberAccess` | `set_space_member_access` |
//! | POST | `/xrpc/town.muni.arbiter.removeSpaceMember` | `remove_space_member` |

#![deny(rust_2018_idioms)]

use std::sync::Arc;

use salvo::conn::tcp::TcpListener;
use salvo::prelude::*;
use salvo::writing::Json;

use arbiter_core3::ArbiterCore;

mod auth;
mod handlers;
mod io;
mod persistence;

use auth::AuthConfig;
use persistence::Persister;

// ---------------------------------------------------------------------------
// CLI / env configuration
// ---------------------------------------------------------------------------

struct AppConfig {
    listen: String,
    hostname: String,
    data_dir: std::path::PathBuf,
    unsafe_auth_token: Option<String>,
    persist_interval_secs: u64,
}

impl AppConfig {
    fn from_env() -> Self {
        Self {
            listen: std::env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            hostname: std::env::var("HOSTNAME")
                .unwrap_or_else(|_| "localhost:8080".to_string()),
            data_dir: std::env::var("DATA_DIR")
                .unwrap_or_else(|_| "./data/arbiters".to_string())
                .into(),
            unsafe_auth_token: std::env::var("UNSAFE_AUTH_TOKEN").ok(),
            persist_interval_secs: std::env::var("PERSIST_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

/// Shared state injected into every request handler.
pub struct ServerState {
    /// The sans-IO core wrapped in a lock for mutation.
    pub core: tokio::sync::Mutex<ArbiterCore>,
    /// Default Rego policy embedded at compile time.
    pub default_policy: &'static str,
    /// The server's own DID (did:web:<hostname>).
    pub server_did: String,
    /// HTTP client for resolving remote arbiter data.
    pub client: reqwest::Client,
    /// Auth configuration.
    pub auth: Arc<AuthConfig>,
}

// ---------------------------------------------------------------------------
// Middleware to inject ServerState
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ServerDataMiddleware {
    state: Arc<ServerState>,
}

#[async_trait]
impl salvo::Handler for ServerDataMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        depot.insert("state", self.state.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = AppConfig::from_env();
    let default_policy = include_str!("../../../policies/arbiter/access-levels.rego");
    let server_did = format!("did:web:{}", config.hostname.replace(':', "%3A"));

    tracing::info!(
        "Starting arbiter server v2 on {} (DID: {})",
        config.listen,
        server_did
    );

    // Set up auth
    let auth = Arc::new(AuthConfig::new().with_unsafe_token_if(config.unsafe_auth_token.clone()));

    // Set up persistence
    let persister = Persister::new(config.data_dir.clone());

    // Create the core with the default policy
    let core = ArbiterCore::new(default_policy);

    // Load existing state from disk
    let core = if let Ok(snapshot) = persister.load_all() {
        tracing::info!("Loaded {} arbiters from disk", snapshot.arbiters.len());
        let mut c = core;
        c.load_snapshot(snapshot);
        c
    } else {
        tracing::info!("No existing state found, starting fresh");
        core
    };

    let state = Arc::new(ServerState {
        core: tokio::sync::Mutex::new(core),
        default_policy,
        server_did,
        client: reqwest::Client::new(),
        auth: auth.clone(),
    });

    // Spawn persistence background task
    {
        let state = state.clone();
        let persister = persister.clone();
        let interval_secs = config.persist_interval_secs;
        tokio::spawn(async move {
            persistence_loop(state, persister, interval_secs).await;
        });
    }

    // Build the router
    let router = build_router(state.clone(), auth);

    // Start serving
    tracing::info!("Listening on {}", config.listen);
    let acceptor = TcpListener::new(&config.listen).bind().await;
    Server::new(acceptor).serve(router).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

fn build_router(
    state: Arc<ServerState>,
    auth_config: Arc<AuthConfig>,
) -> Router {
    let auth_middleware = auth::AuthMiddleware::new(auth_config);

    Router::new()
        .hoop(ServerDataMiddleware { state: state.clone() })
        // Create
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::create_arbiter),
        )
        // Config
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getArbiterConfig")
                .hoop(auth_middleware.clone())
                .get(handlers::get_arbiter_config),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setArbiterConfig")
                .hoop(auth_middleware.clone())
                .post(handlers::set_arbiter_config),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_arbiter),
        )
        // Spaces
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::create_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getSpaceConfig")
                .hoop(auth_middleware.clone())
                .get(handlers::get_space_config),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setSpaceConfig")
                .hoop(auth_middleware.clone())
                .post(handlers::set_space_config),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.listSpaces")
                .hoop(auth_middleware.clone())
                .get(handlers::list_spaces),
        )
        // Members
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getSpaceMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::get_space_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.resolveSpaceMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::resolve_space_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setSpaceMemberAccess")
                .hoop(auth_middleware.clone())
                .post(handlers::set_space_member_access),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.removeSpaceMember")
                .hoop(auth_middleware)
                .post(handlers::remove_space_member),
        )
        // Health
        .push(Router::with_path("/").get(index))
}

#[handler]
async fn index(res: &mut Response) {
    res.render(Json(serde_json::json!({
        "service": "muni-town-arbiter",
        "version": "2",
        "serverDid": "",
    })));
}

// ---------------------------------------------------------------------------
// Persistence loop
// ---------------------------------------------------------------------------

async fn persistence_loop(
    state: Arc<ServerState>,
    persister: Persister,
    interval_secs: u64,
) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

        let snapshot = {
            let core = state.core.lock().await;
            core.snapshot()
        };

        if let Err(e) = persister.save_all(&snapshot) {
            tracing::error!("Failed to persist state: {e}");
        } else {
            tracing::debug!("Persisted {} arbiters", snapshot.arbiters.len());
        }
    }
}
