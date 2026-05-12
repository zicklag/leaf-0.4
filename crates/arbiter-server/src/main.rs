//! Entry point for the Muni Town Arbiter Server.
//!
//! Sets up the Salvo HTTP server with all XRPC endpoints, auth middleware,
//! persistence, and background tasks.

use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;

use salvo::conn::tcp::TcpListener;
use salvo::prelude::*;
use salvo::Router;

use arbiter_core::futures::AsyncArbiterServer;

mod auth;
mod handlers;
mod io;
mod persistence;

use auth::AuthConfig;
use io::{HttpArbiterIo, PlcDidResolver};
use persistence::Persister;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// Configuration from CLI and environment.
struct AppConfig {
    /// Address to listen on.
    listen: String,
    /// Directory to store arbiter state files.
    data_dir: PathBuf,
    /// DID of this server instance.
    server_did: String,
    /// PLC directory URL for DID resolution.
    plc_url: String,
    /// Optional unsafe auth token for development.
    unsafe_auth_token: Option<String>,
    /// Interval between persistence flushes.
    persist_interval: Duration,
    /// Tick interval for the background task.
    tick_interval: Duration,
}

impl AppConfig {
    /// Load configuration from environment variables and defaults.
    fn from_env() -> Self {
        Self {
            listen: std::env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            data_dir: PathBuf::from(
                std::env::var("DATA_DIR").unwrap_or_else(|_| "./data/arbiters".to_string()),
            ),
            server_did: std::env::var("SERVER_DID")
                .expect("SERVER_DID environment variable is required"),
            plc_url: std::env::var("PLC_URL")
                .unwrap_or_else(|_| "https://plc.directory".to_string()),
            unsafe_auth_token: std::env::var("UNSAFE_AUTH_TOKEN").ok(),
            persist_interval: Duration::from_secs(
                std::env::var("PERSIST_INTERVAL")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5),
            ),
            tick_interval: Duration::from_millis(
                std::env::var("TICK_INTERVAL_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared server data
// ---------------------------------------------------------------------------

/// Wrapper to hold the server in the depot via middleware.
#[derive(Clone)]
struct ServerData {
    server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
}

/// Middleware that injects shared server data into the depot.
struct ServerDataMiddleware {
    data: ServerData,
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
        depot.insert("server", self.data.server.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = AppConfig::from_env();

    tracing::info!(
        "Starting arbiter server on {} (DID: {})",
        config.listen,
        config.server_did
    );

    // Create HTTP client for outbound requests
    let client = reqwest::Client::new();

    // Set up DID resolver
    let did_resolver = Arc::new(PlcDidResolver::new(config.plc_url.clone(), client.clone()));

    // Create the auth token for outbound requests (our server DID)
    let auth_token = config
        .unsafe_auth_token
        .clone()
        .unwrap_or_else(|| config.server_did.clone());

    // Set up IO layer
    let io = HttpArbiterIo::new(client, did_resolver, auth_token);

    // Create the async arbiter server
    let arbiter_server = AsyncArbiterServer::with_tick_interval(io, config.tick_interval);

    // Set up persistence
    let persister = Arc::new(Persister::new(config.data_dir.clone()));

    // Load existing arbiter states from disk
    let loaded = persister.load_all();
    if !loaded.is_empty() {
        tracing::info!("Loaded {} arbiter states from disk", loaded.len());
        arbiter_server.load_all(loaded).await;
    }

    // Spawn background tick task
    {
        let server = arbiter_server.clone();
        tokio::spawn(async move {
            server.background_task().await;
        });
        tracing::info!("Background tick task started");
    }

    // Spawn persistence task
    {
        let server = arbiter_server.clone();
        let persister = persister.clone();
        let interval = config.persist_interval;
        tokio::spawn(async move {
            persistence_loop(server, persister, interval).await;
        });
        tracing::info!("Persistence task started (interval: {:?})", interval);
    }

    // Build auth config
    let auth_config = Arc::new(
        if let Some(token) = config.unsafe_auth_token {
            AuthConfig::new(config.server_did).with_unsafe_token(token)
        } else {
            AuthConfig::new(config.server_did)
        },
    );

    // Build the router
    let router = build_router(arbiter_server.clone(), auth_config);

    // Start serving
    tracing::info!("Listening on {}", config.listen);
    let acceptor = TcpListener::new(&config.listen).bind().await;
    Server::new(acceptor).serve(router).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Build the Salvo router with all XRPC endpoints.
fn build_router(
    server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
    auth_config: Arc<AuthConfig>,
) -> Router {
    let auth_middleware = auth::AuthMiddleware::new(auth_config);
    let server_data = ServerData { server };
    let server_middleware = ServerDataMiddleware { data: server_data };

    Router::new()
        .hoop(server_middleware)
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::create_arbiter),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteArbiter")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_arbiter),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::get_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.resolveMembers")
                .hoop(auth_middleware.clone())
                .get(handlers::resolve_members),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::create_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::delete_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.configureSpace")
                .hoop(auth_middleware.clone())
                .post(handlers::configure_space),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setMemberAccess")
                .hoop(auth_middleware.clone())
                .post(handlers::set_member_access),
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.removeMember")
                .hoop(auth_middleware)
                .post(handlers::remove_member),
        )
        .push(
            Router::with_path("/")
                .get(index),
        )
}

/// Root endpoint handler.
#[handler]
async fn index(res: &mut Response) {
    res.render("Arbiter Server is running");
}

// ---------------------------------------------------------------------------
// Persistence loop
// ---------------------------------------------------------------------------

/// Periodically flush dirty arbiter states to disk.
async fn persistence_loop(
    arbiter_server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
    persister: Arc<Persister>,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;

        let dirty = arbiter_server.drain_dirty_arbiters().await;
        if dirty.is_empty() {
            continue;
        }

        for did in &dirty {
            if let Some(state) = arbiter_server.snapshot_arbiter(did).await {
                if let Err(e) = persister.persist(did, &state) {
                    tracing::error!(%did, %e, "Failed to persist arbiter state");
                }
            }
        }

        tracing::debug!("Persisted {} dirty arbiter(s)", dirty.len());
    }
}
